//! The application: state, the event loop and background tasks.
//!
//! Screens turn key presses into [`Action`]s. Actions that need the network
//! run as tasks that report back with a [`Msg`], so the UI never blocks.

use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::{DefaultTerminal, Frame};
use stravacz_core::api::{self, Client, Credentials, Menu, Session, Submission};
use stravacz_core::secrets::{self, SecretError};
use stravacz_core::{ConfigStore, SavedLogin, remember_login};
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::time::MissedTickBehavior;

use crate::screens::{LoginScreen, MenuScreen};
use crate::widgets::{Toast, popup};

const TICK: Duration = Duration::from_millis(80);
const SPINNER: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Something a screen asks the application to do.
#[derive(Debug)]
pub enum Action {
    Login {
        credentials: Credentials,
        remember: bool,
    },
    Refresh,
    Submit,
    Logout,
    Help,
    Quit,
    Notify(Toast),
}

/// The result of a background task.
#[derive(Debug)]
enum Msg {
    StoredPassword(Result<Option<String>, SecretError>),
    LoggedIn(api::Result<(Session, Menu)>),
    Refreshed(api::Result<Menu>),
    Submitted(api::Result<Submission>),
    Warning(String),
}

#[derive(Debug)]
enum Screen {
    Login(LoginScreen),
    Menu(Box<MenuScreen>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Help,
    ConfirmQuit,
}

/// Run the UI until the user quits.
pub async fn run(mut terminal: DefaultTerminal) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let mut app = App::new(tx)?;
    let mut events = EventStream::new();
    let mut ticker = tokio::time::interval(TICK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);

    app.restore_saved_login();
    while app.running {
        terminal.draw(|frame| app.draw(frame))?;
        tokio::select! {
            event = events.next() => match event {
                Some(event) => app.on_event(&event?),
                None => break,
            },
            Some(msg) = rx.recv() => app.on_message(msg),
            _ = ticker.tick() => app.on_tick(),
        }
    }

    if let Screen::Menu(screen) = &app.screen {
        // Best effort: do not keep the user waiting on a slow server.
        let _ = tokio::time::timeout(Duration::from_secs(2), screen.session().logout()).await;
    }
    Ok(())
}

#[derive(Debug)]
struct App {
    client: Client,
    config: ConfigStore,
    screen: Screen,
    overlay: Overlay,
    toast: Option<Toast>,
    /// What is running in the background, if anything.
    busy: Option<&'static str>,
    tick: usize,
    tx: UnboundedSender<Msg>,
    running: bool,
}

impl App {
    fn new(tx: UnboundedSender<Msg>) -> Result<Self> {
        let config = ConfigStore::new()?;
        let (saved, toast) = match config.load() {
            Ok(loaded) => (loaded.login, None),
            Err(err) => (None, Some(Toast::error(err.to_string()))),
        };

        Ok(Self {
            client: match std::env::var("STRAVA_BASE_URL") {
                Ok(url) => Client::with_base_url(&url)?,
                Err(_) => Client::new()?,
            },
            config,
            screen: Screen::Login(LoginScreen::new(saved.as_ref())),
            overlay: Overlay::None,
            toast,
            busy: None,
            tick: 0,
            tx,
            running: true,
        })
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let busy = self
            .busy
            .map(|label| format!("{} {label}", SPINNER[self.tick % SPINNER.len()]));

        match &mut self.screen {
            Screen::Login(screen) => screen.render(frame, area, busy.as_deref()),
            Screen::Menu(screen) => screen.render(frame, area, busy.as_deref()),
        }
        if let Some(toast) = &self.toast {
            toast.render(frame, area);
        }
        match (self.overlay, &self.screen) {
            (Overlay::Help, _) => popup::help(frame, area),
            (Overlay::ConfirmQuit, Screen::Menu(screen)) => {
                popup::confirm_quit(frame, area, screen.draft().len());
            }
            _ => {}
        }
    }

    fn on_tick(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        if self.toast.as_ref().is_some_and(Toast::is_expired) {
            self.toast = None;
        }
    }

    fn on_event(&mut self, event: &Event) {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            self.on_key(*key);
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.running = false;
            return;
        }

        match self.overlay {
            Overlay::Help => self.overlay = Overlay::None,
            Overlay::ConfirmQuit => {
                self.overlay = Overlay::None;
                if matches!(key.code, KeyCode::Char('y' | 'q') | KeyCode::Enter) {
                    self.running = false;
                }
            }
            Overlay::None => {
                let action = match &mut self.screen {
                    // Keep the form steady while signing in.
                    Screen::Login(_) if self.busy.is_some() => {
                        (key.code == KeyCode::Esc).then_some(Action::Quit)
                    }
                    Screen::Login(screen) => screen.handle_key(key),
                    Screen::Menu(screen) => screen.handle_key(key),
                };
                if let Some(action) = action {
                    self.perform(action);
                }
            }
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::Quit => match &self.screen {
                Screen::Menu(screen) if !screen.draft().is_empty() => {
                    self.overlay = Overlay::ConfirmQuit;
                }
                _ => self.running = false,
            },
            Action::Help => self.overlay = Overlay::Help,
            Action::Notify(toast) => self.toast = Some(toast),
            Action::Login {
                credentials,
                remember,
            } => self.login(credentials, remember),
            Action::Refresh => {
                if let Some(session) = self.start("Refreshing…") {
                    self.spawn(async move { Msg::Refreshed(session.menu().await) });
                }
            }
            Action::Submit => self.submit(),
            Action::Logout => self.logout(),
        }
    }

    /// Mark the app busy and hand out the session, unless something is running.
    fn start(&mut self, label: &'static str) -> Option<Session> {
        let Screen::Menu(screen) = &self.screen else {
            return None;
        };
        if self.busy.is_some() {
            self.toast = Some(Toast::info("Hold on, still working…"));
            return None;
        }
        self.busy = Some(label);
        Some(screen.session().clone())
    }

    fn spawn(&self, task: impl Future<Output = Msg> + Send + 'static) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            // The receiver only goes away when the app is shutting down.
            let _ = tx.send(task.await);
        });
    }

    /// Log in automatically if the password was saved last time.
    fn restore_saved_login(&mut self) {
        let Ok(config) = self.config.load() else {
            return;
        };
        let Some(saved) = config.login.filter(|login| login.remember_password) else {
            return;
        };

        self.busy = Some("Signing in…");
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(Msg::StoredPassword(secrets::load_password(&saved)));
        });
    }

    fn login(&mut self, credentials: Credentials, remember: bool) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some("Signing in…");

        let client = self.client.clone();
        let config = self.config.clone();
        let tx = self.tx.clone();
        self.spawn(async move {
            let result = async {
                let session = client.login(&credentials).await?;
                let menu = session.menu().await?;
                Ok((session, menu))
            }
            .await;

            // Only remember credentials that actually worked.
            if result.is_ok() {
                let login = SavedLogin {
                    canteen: credentials.canteen,
                    username: credentials.username,
                    remember_password: remember,
                };
                let password = remember.then_some(credentials.password);
                let saved = tokio::task::spawn_blocking(move || {
                    remember_login(&config, login, password.as_deref())
                })
                .await;
                if let Ok(Err(err)) = saved {
                    let _ = tx.send(Msg::Warning(format!("Couldn't remember the login: {err}")));
                }
            }
            Msg::LoggedIn(result)
        });
    }

    fn submit(&mut self) {
        let changes = match &self.screen {
            Screen::Menu(screen) => screen.draft().changes(),
            Screen::Login(_) => return,
        };
        if changes.is_empty() {
            self.toast = Some(Toast::info("Nothing to save."));
            return;
        }
        if let Some(session) = self.start("Saving orders…") {
            self.spawn(async move { Msg::Submitted(session.submit(&changes).await) });
        }
    }

    fn logout(&mut self) {
        let Screen::Menu(screen) = &self.screen else {
            return;
        };
        let session = screen.session().clone();
        tokio::spawn(async move {
            if let Err(err) = session.logout().await {
                tracing::warn!(%err, "logout failed");
            }
        });
        self.show_login(None);
        self.toast = Some(Toast::info("Logged out."));
    }

    fn show_login(&mut self, error: Option<String>) {
        let saved = self.config.load().ok().and_then(|config| config.login);
        let mut screen = LoginScreen::new(saved.as_ref());
        if let Some(error) = error {
            screen.set_error(error);
        }
        self.screen = Screen::Login(screen);
        self.overlay = Overlay::None;
    }

    fn on_message(&mut self, msg: Msg) {
        match msg {
            Msg::StoredPassword(result) => {
                self.busy = None;
                match result {
                    Ok(Some(password)) => {
                        if let Screen::Login(screen) = &mut self.screen
                            && let Some(action) = screen.autofill(password)
                        {
                            self.perform(action);
                        }
                    }
                    Ok(None) => {}
                    Err(err) => {
                        self.toast = Some(Toast::warning(format!(
                            "Couldn't read the saved password: {err}"
                        )));
                    }
                }
            }
            Msg::LoggedIn(result) => {
                self.busy = None;
                match result {
                    Ok((session, menu)) => {
                        let name = session.account().full_name.clone();
                        self.screen = Screen::Menu(Box::new(MenuScreen::new(session, menu)));
                        self.toast = Some(Toast::success(format!("Welcome, {name}!")));
                    }
                    Err(err) => {
                        tracing::warn!(%err, "login failed");
                        if let Screen::Login(screen) = &mut self.screen {
                            screen.set_error(err.to_string());
                        }
                    }
                }
            }
            Msg::Refreshed(result) => {
                self.busy = None;
                match (result, &mut self.screen) {
                    (Ok(menu), Screen::Menu(screen)) => {
                        screen.set_menu(menu);
                        self.toast = Some(Toast::info("Menu reloaded."));
                    }
                    (Err(err), _) => self.on_api_error(&err),
                    (Ok(_), Screen::Login(_)) => {}
                }
            }
            Msg::Submitted(result) => {
                self.busy = None;
                match (result, &mut self.screen) {
                    (Ok(submission), Screen::Menu(screen)) => {
                        self.toast = Some(screen.finish_submit(submission));
                    }
                    (Err(err), _) => self.on_api_error(&err),
                    (Ok(_), Screen::Login(_)) => {}
                }
            }
            Msg::Warning(warning) => self.toast = Some(Toast::warning(warning)),
        }
    }

    fn on_api_error(&mut self, err: &api::Error) {
        tracing::warn!(%err, "request failed");
        if err.is_authentication() && matches!(self.screen, Screen::Menu(_)) {
            self.show_login(Some(format!("Session expired: {err}")));
        } else {
            self.toast = Some(Toast::error(err.to_string()));
        }
    }
}
