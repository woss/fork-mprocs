use tokio::sync::mpsc::UnboundedReceiver;

use super::layout::{Dir, Layout, PaneId, SizeSpec};
use super::modals::quit_modal::QuitModal;
use super::modals::{Modal, ModalAction};
use super::views::procs::ProcsPane;
use super::views::term::TermPane;
use crate::color;
use crate::console::state::{ConsoleState, ConsoleTaskEntry};
use crate::mprocs::app::{ClientHandle, ClientId};
use crate::{
  error::ResultLogger,
  kernel::kernel_message::{
    KernelCommand, KernelQuery, KernelQueryResponse, TaskContext,
  },
  kernel::task::{
    TaskCmd, TaskDef, TaskId, TaskNotification, TaskNotify, TaskStatus,
  },
  protocol::{CltToSrv, SrvToClt},
  server::server_message::ServerMessage,
  term::{
    Grid, Size, TermEvent,
    attrs::Attrs,
    grid::Rect,
    key::{KeyCode, KeyEventKind, KeyMods},
  },
};

struct Console {
  task_context: TaskContext,
  receiver: UnboundedReceiver<TaskCmd>,
  clients: Vec<ClientHandle>,
  grid: Grid,
  screen_size: Size,
  layout: Layout,
  focused_pane: PaneId,

  state: ConsoleState,
}

impl Console {
  async fn run(mut self) {
    self.task_context.send(KernelCommand::ListenTaskUpdates);
    self.refresh_tasks().await;

    let mut render_needed = true;
    let mut command_buf = Vec::new();

    loop {
      if render_needed && !self.clients.is_empty() {
        self.render().await;
        render_needed = false;
      }

      if self.receiver.recv_many(&mut command_buf, 512).await == 0 {
        break;
      }
      for cmd in command_buf.drain(..) {
        if self.handle_cmd(cmd).await {
          render_needed = true;
        }
      }
    }
  }

  async fn handle_cmd(&mut self, cmd: TaskCmd) -> bool {
    match cmd {
      TaskCmd::Msg(msg) => {
        let msg = match msg.downcast::<ServerMessage>() {
          Ok(server_msg) => return self.handle_server_msg(*server_msg).await,
          Err(msg) => msg,
        };
        if let Ok(n) = msg.downcast::<TaskNotification>() {
          return self.handle_notification(n.from, n.notify);
        }
        false
      }
      _ => false,
    }
  }

  fn handle_notification(&mut self, from: TaskId, notify: TaskNotify) -> bool {
    match notify {
      TaskNotify::Added(path, status, vt) => {
        let path = path
          .map(|p| p.to_string())
          .unwrap_or_else(|| format!("<task:{}>", from.0));
        self.state.tasks.push(ConsoleTaskEntry {
          id: from,
          path,
          status,
          vt,
        });
        true
      }
      TaskNotify::Started => {
        if let Some(entry) = self.state.tasks.iter_mut().find(|t| t.id == from)
        {
          entry.status = TaskStatus::Running;
        }
        true
      }
      TaskNotify::Stopped(_) => {
        if let Some(entry) = self.state.tasks.iter_mut().find(|t| t.id == from)
        {
          entry.status = TaskStatus::Down;
        }
        true
      }
      TaskNotify::Removed => {
        self.state.tasks.retain(|t| t.id != from);
        if self.state.selected >= self.state.tasks.len()
          && !self.state.tasks.is_empty()
        {
          self.state.selected = self.state.tasks.len() - 1;
        }
        true
      }
      TaskNotify::Rendered => true,
    }
  }

  async fn handle_server_msg(&mut self, msg: ServerMessage) -> bool {
    match msg {
      ServerMessage::ClientConnected { handle } => {
        self.clients.push(handle);
        self.update_screen_size();
        true
      }
      ServerMessage::ClientDisconnected { client_id } => {
        self.clients.retain(|c| c.id != client_id);
        self.update_screen_size();
        true
      }
      ServerMessage::ClientMessage { client_id, msg } => match msg {
        CltToSrv::Key(event) => self.handle_key(client_id, event).await,
        CltToSrv::Init { width, height } => {
          self.screen_size = Size { width, height };
          self.grid.set_size(self.screen_size);
          true
        }
        CltToSrv::Rpc(_) => false,
      },
    }
  }

  async fn handle_key(
    &mut self,
    client_id: ClientId,
    event: TermEvent,
  ) -> bool {
    let key = match event {
      TermEvent::Key(k) if k.kind != KeyEventKind::Release => k,
      _ => return false,
    };

    if self.state.quit_modal {
      let action = QuitModal.handle_key(key, &mut self.state);
      match action {
        ModalAction::None => {}
        ModalAction::Detach => {
          if let Some(client) =
            self.clients.iter_mut().find(|c| c.id == client_id)
          {
            let _ = client.sender.send(SrvToClt::Quit).await;
          }
        }
        ModalAction::Quit => {
          if let Some(client) =
            self.clients.iter_mut().find(|c| c.id == client_id)
          {
            self.task_context.send(KernelCommand::Quit);
            let _ = client.sender.send(SrvToClt::Quit).await;
          }
        }
      }
      return true;
    }

    let nav_mods = key.mods == KeyMods::CONTROL
      || key.mods == KeyMods::CONTROL | KeyMods::ALT;

    match key.code {
      KeyCode::Char('j') | KeyCode::Down if key.mods == KeyMods::NONE => {
        self.move_selection(1);
        true
      }
      KeyCode::Char('k') | KeyCode::Up if key.mods == KeyMods::NONE => {
        self.move_selection(-1);
        true
      }
      KeyCode::Char('h') if nav_mods => self.focus_neighbor(Dir::Left),
      KeyCode::Char('j') if nav_mods => self.focus_neighbor(Dir::Down),
      KeyCode::Char('k') if nav_mods => self.focus_neighbor(Dir::Up),
      KeyCode::Char('l') if nav_mods => self.focus_neighbor(Dir::Right),
      KeyCode::Char('q') if key.mods == KeyMods::NONE => {
        self.state.quit_modal = true;
        true
      }
      _ => false,
    }
  }

  fn focus_neighbor(&mut self, dir: Dir) -> bool {
    if let Some(next) = self.layout.neighbor(self.focused_pane, dir) {
      if next != self.focused_pane {
        self.focused_pane = next;
        return true;
      }
    }
    false
  }

  fn move_selection(&mut self, delta: i32) {
    if self.state.tasks.is_empty() {
      return;
    }
    let len = self.state.tasks.len() as i32;
    let new = (self.state.selected as i32 + delta).rem_euclid(len);
    self.state.selected = new as usize;
  }

  async fn refresh_tasks(&mut self) {
    let rx = self.task_context.query(KernelQuery::ListTasks(None));
    if let Ok(KernelQueryResponse::TaskList(list)) = rx.await {
      self.state.tasks = list
        .into_iter()
        .map(|t| ConsoleTaskEntry {
          id: t.id,
          path: t
            .path
            .map(|p| p.to_string())
            .unwrap_or_else(|| format!("<task:{}>", t.id.0)),
          status: t.status,
          vt: t.vt,
        })
        .collect();
      if self.state.selected >= self.state.tasks.len()
        && !self.state.tasks.is_empty()
      {
        self.state.selected = self.state.tasks.len() - 1;
      }
    }
  }

  fn update_screen_size(&mut self) {
    if let Some(client) = self.clients.first() {
      self.screen_size = client.size();
      self.grid.set_size(self.screen_size);
    }
  }

  async fn render(&mut self) {
    let def_attrs =
      Attrs::default().fg(color!("#e0e0e0")).bg(color!("#111111"));

    let grid = &mut self.grid;
    grid.erase_all(def_attrs);
    grid.cursor_pos = None;

    let area = Rect::new(0, 0, self.screen_size.width, self.screen_size.height);
    if area.width < 4 || area.height < 3 {
      return;
    }

    let (title_row, area) = area.split_h(1);
    let (body, help_row) = area.split_h(area.height - 1);

    // Title bar
    let logo_attrs = Attrs::default()
      .fg(color!("#000000"))
      .bg(color!("#69e8ff"))
      .set_bold(true);
    grid.draw_text(title_row, " dekit ", logo_attrs);
    let bar_attrs = Attrs::default()
      .fg(color!("#69e8ff"))
      .bg(color!("#d0d0d0"))
      .set_bold(true);
    grid.draw_line(title_row.move_left(7), "\u{e0bc} ", bar_attrs);

    self.layout.resize(body.size());
    let geometry = self.layout.render();
    for (id, local) in geometry {
      let area = Rect::new(
        body.x + local.x,
        body.y + local.y,
        local.width,
        local.height,
      );
      self.layout.pane_mut(id).render(
        grid,
        area,
        &mut self.state,
        id == self.focused_pane,
      );
    }

    // Bottom help bar
    let help_bg = def_attrs;
    grid.fill_area(help_row, ' ', help_bg);
    let bindings: &[(&str, &str)] =
      &[("`", "leader"), ("C-h/j/k/l", "select pane")];
    let mut cursor = Rect::new(help_row.x + 1, help_row.y, help_row.width, 1);
    let key_attrs = def_attrs.clone().fg(color!("#7da8e8")).set_bold(true);
    let desc_attrs = def_attrs.clone().fg(color!("#dddddd"));
    let sep_attrs = def_attrs.clone().fg(color!("#888888"));
    for (i, (key, desc)) in bindings.into_iter().enumerate() {
      if i > 0 {
        let used = grid.draw_text(cursor, " \u{00b7} ", sep_attrs);
        cursor.x = used.right();
      }

      let used = grid.draw_text(cursor, &format!("{}", key), key_attrs);
      cursor.x = used.right();
      let used = grid.draw_text(cursor, &format!(" {}", desc), desc_attrs);
      cursor.x = used.right();
    }

    if self.state.quit_modal {
      QuitModal.draw(grid);
    }

    // Send diffs to clients
    for client in &mut self.clients {
      let mut out = String::new();
      client.differ.diff(&mut out, grid).log_ignore();
      let _ = client.sender.send(SrvToClt::Print(out)).await;
      let _ = client.sender.send(SrvToClt::Flush).await;
    }
  }
}

pub fn create_console_task(pc: &TaskContext) -> TaskId {
  pc.spawn_async(
    TaskDef {
      status: TaskStatus::Running,
      ..Default::default()
    },
    |pc, receiver| async move {
      log::debug!("Creating console task (id: {})", pc.task_id.0);
      let initial_size = Size {
        width: 80,
        height: 24,
      };
      let mut layout = Layout::new(initial_size);
      let root = layout.root();
      let procs_pane = layout.insert(
        root,
        Dir::Right,
        Box::new(ProcsPane),
        SizeSpec::Fixed(30),
      );
      layout.insert(root, Dir::Right, Box::new(TermPane), SizeSpec::Fill);
      let app = Console {
        task_context: pc,
        receiver,
        clients: Vec::new(),
        grid: Grid::new(initial_size, 0),
        screen_size: initial_size,
        layout,
        focused_pane: procs_pane,
        state: ConsoleState {
          tasks: Vec::new(),
          selected: 0,
          quit_modal: false,
        },
      };
      app.run().await;
    },
  )
}
