//! The application: state, the event loop and background tasks.
//!
//! Screens turn key presses into [`Action`]s. Actions that need the network
//! run as tasks that report back with a [`Msg`], so the UI never blocks.

use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::{DefaultTerminal, Frame};
use stravacz_core::api::{self, Account, Client, Credentials, Menu, Session, Submission};
use stravacz_core::secrets::{self, SecretError};
use stravacz_core::{ConfigStore, OrderDraft, SavedLogin, remember_login};
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

/// Work running in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Task {
    SigningIn,
    Refreshing,
    Saving,
}

impl Task {
    fn label(self) -> &'static str {
        match self {
            Self::SigningIn => "Signing in…",
            Self::Refreshing => "Refreshing…",
            Self::Saving => "Saving orders…",
        }
    }
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
    busy: Option<Task>,
    /// Pending changes from a session that expired, for when the same
    /// account logs in again.
    resume: Option<Resume>,
    tick: usize,
    tx: UnboundedSender<Msg>,
    running: bool,
}

#[derive(Debug)]
struct Resume {
    canteen: String,
    username: String,
    draft: OrderDraft,
}

impl Resume {
    fn is_for(&self, account: &Account) -> bool {
        self.canteen == account.canteen_number && self.username == account.username
    }
}

impl App {
    fn new(tx: UnboundedSender<Msg>) -> Result<Self> {
        let client = match std::env::var("STRAVA_BASE_URL") {
            Ok(url) => Client::with_base_url(&url)?,
            Err(_) => Client::new()?,
        };
        Ok(Self::with(client, ConfigStore::new()?, tx))
    }

    fn with(client: Client, config: ConfigStore, tx: UnboundedSender<Msg>) -> Self {
        let (saved, toast) = match config.load() {
            Ok(loaded) => (loaded.login, None),
            Err(err) => (None, Some(Toast::error(err.to_string()))),
        };

        Self {
            client,
            config,
            screen: Screen::Login(LoginScreen::new(saved.as_ref())),
            overlay: Overlay::None,
            toast,
            busy: None,
            resume: None,
            tick: 0,
            tx,
            running: true,
        }
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let busy = self
            .busy
            .map(|task| format!("{} {}", SPINNER[self.tick % SPINNER.len()], task.label()));

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
            // Like `q`, but a second ctrl-c confirms quitting with pending changes.
            if self.overlay == Overlay::ConfirmQuit {
                self.running = false;
            } else {
                self.overlay = Overlay::None;
                self.perform(Action::Quit);
            }
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
            // Quitting mid-save would skip discarding half-staged orders.
            Action::Quit if self.busy == Some(Task::Saving) => {
                self.toast = Some(Toast::info("Saving orders, quit once it's done…"));
            }
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
                if let Some(session) = self.start(Task::Refreshing) {
                    self.spawn(async move { Msg::Refreshed(session.menu().await) });
                }
            }
            Action::Submit => self.submit(),
            Action::Logout => self.logout(),
        }
    }

    /// Mark the app busy and hand out the session, unless something is running.
    fn start(&mut self, task: Task) -> Option<Session> {
        let Screen::Menu(screen) = &self.screen else {
            return None;
        };
        let session = screen.session().clone();
        if self.refuse_while_busy() {
            return None;
        }
        self.busy = Some(task);
        Some(session)
    }

    /// Tell the user to wait if something is running; returns whether it is.
    fn refuse_while_busy(&mut self) -> bool {
        if self.busy.is_some() {
            self.toast = Some(Toast::info("Hold on, still working…"));
        }
        self.busy.is_some()
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

        self.busy = Some(Task::SigningIn);
        let tx = self.tx.clone();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(Msg::StoredPassword(secrets::load_password(&saved)));
        });
    }

    fn login(&mut self, credentials: Credentials, remember: bool) {
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(Task::SigningIn);

        let client = self.client.clone();
        let config = self.config.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let result = async {
                let session = client.login(&credentials).await?;
                let menu = session.menu().await?;
                Ok((session, menu))
            }
            .await;
            let worked = result.is_ok();
            let _ = tx.send(Msg::LoggedIn(result));

            // Only remember credentials that actually worked. This comes after
            // showing the menu, as the keyring may be slow or ask to be unlocked.
            if worked {
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
        if let Some(session) = self.start(Task::Saving) {
            self.spawn(async move { Msg::Submitted(session.submit(&changes).await) });
        }
    }

    fn logout(&mut self) {
        // Logging out mid-request would race it, e.g. half-apply a submission.
        if self.refuse_while_busy() {
            return;
        }
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
                        let resume = self
                            .resume
                            .take()
                            .filter(|resume| resume.is_for(session.account()));
                        let name = session.account().full_name.clone();
                        let mut screen = MenuScreen::new(session, menu);
                        self.toast = Some(Toast::success(match resume {
                            Some(resume) => {
                                screen.restore_draft(resume.draft);
                                match screen.draft().len() {
                                    0 => "Logged in again.".to_owned(),
                                    1 => "Logged in again; your pending change is kept.".to_owned(),
                                    n => format!(
                                        "Logged in again; your {n} pending changes are kept."
                                    ),
                                }
                            }
                            None => format!("Welcome, {name}!"),
                        }));
                        self.screen = Screen::Menu(Box::new(screen));
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
        if err.is_authentication()
            && let Screen::Menu(screen) = &self.screen
        {
            if !screen.draft().is_empty() {
                let account = screen.session().account();
                self.resume = Some(Resume {
                    canteen: account.canteen_number.clone(),
                    username: account.username.clone(),
                    draft: screen.draft().clone(),
                });
            }
            self.show_login(Some(format!("Session expired: {err}")));
            self.restore_saved_login();
        } else {
            self.toast = Some(Toast::error(err.to_string()));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use stravacz_core::api::mock;
    use tokio::sync::mpsc::UnboundedReceiver;
    use wiremock::MockServer;

    use super::*;

    /// An app talking to the mock canteen, with its own config file.
    struct Harness {
        app: App,
        rx: UnboundedReceiver<Msg>,
        server: MockServer,
        dir: PathBuf,
    }

    impl Harness {
        async fn new(name: &str) -> Self {
            let server = mock::start().await;
            let dir = std::env::temp_dir()
                .join(format!("stravacz-tui-test-{}-{name}", std::process::id()));
            let (tx, rx) = mpsc::unbounded_channel();
            let client = Client::with_base_url(&server.uri()).unwrap();
            let app = App::with(client, ConfigStore::at(dir.join("config.toml")), tx);
            Self {
                app,
                rx,
                server,
                dir,
            }
        }

        /// Log in without touching the keyring.
        async fn logged_in(name: &str) -> Self {
            let mut harness = Self::new(name).await;
            harness.login("jan.novak").await;
            harness
        }

        async fn login(&mut self, username: &str) {
            self.app.perform(Action::Login {
                credentials: Credentials::new("1234", username, "secret"),
                remember: false,
            });
            self.settle().await;
            assert!(matches!(self.app.screen, Screen::Menu(_)), "logged in");
        }

        /// Handle background results until nothing is running.
        async fn settle(&mut self) {
            while self.app.busy.is_some() {
                let msg = tokio::time::timeout(Duration::from_secs(5), self.rx.recv())
                    .await
                    .expect("a background task finishes")
                    .expect("the channel is open");
                self.app.on_message(msg);
            }
        }

        fn press(&mut self, code: KeyCode) {
            self.app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
        }

        fn menu(&self) -> &MenuScreen {
            match &self.app.screen {
                Screen::Menu(screen) => screen,
                Screen::Login(_) => panic!("not on the menu screen"),
            }
        }

        /// Stage ordering a lunch on the next day that allows it.
        fn pick_lunch(&mut self) {
            for _ in 0..10 {
                self.press(KeyCode::Char('l'));
                self.press(KeyCode::Char('j'));
                self.press(KeyCode::Char(' '));
                if !self.menu().draft().is_empty() {
                    return;
                }
            }
            panic!("no orderable lunch on the mock menu");
        }

        fn toast(&self) -> &str {
            self.app.toast.as_ref().map_or("", Toast::message)
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[tokio::test]
    async fn login_shows_the_menu() {
        let mut harness = Harness::logged_in("login").await;
        assert!(
            harness.toast().starts_with("Welcome"),
            "{}",
            harness.toast()
        );

        harness.pick_lunch();
        harness.press(KeyCode::Char('s'));
        harness.settle().await;
        assert!(harness.menu().draft().is_empty());
        assert_eq!(harness.toast(), "Order saved.");
    }

    #[tokio::test]
    async fn logout_waits_for_a_running_save() {
        let mut harness = Harness::logged_in("logout").await;
        harness.pick_lunch();
        harness.press(KeyCode::Char('s'));

        harness.press(KeyCode::Char('L'));
        assert!(matches!(harness.app.screen, Screen::Menu(_)));
        assert_eq!(harness.toast(), "Hold on, still working…");

        harness.settle().await;
        assert!(harness.menu().draft().is_empty(), "the save went through");
        harness.press(KeyCode::Char('L'));
        assert!(matches!(harness.app.screen, Screen::Login(_)));
    }

    #[tokio::test]
    async fn ctrl_c_waits_for_a_running_save() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut harness = Harness::logged_in("ctrl-c-save").await;
        harness.pick_lunch();
        harness.press(KeyCode::Char('s'));

        harness.app.on_key(ctrl_c);
        harness.press(KeyCode::Char('q'));
        assert!(harness.app.running);
        assert_eq!(harness.app.overlay, Overlay::None);

        harness.settle().await;
        harness.app.on_key(ctrl_c);
        assert!(!harness.app.running);
    }

    #[tokio::test]
    async fn expired_session_keeps_pending_changes() {
        let mut harness = Harness::logged_in("expired").await;
        harness.pick_lunch();
        let changes = harness.menu().draft().changes();

        mock::expire_session(&harness.server).await;
        harness.press(KeyCode::Char('r'));
        harness.settle().await;
        assert!(matches!(harness.app.screen, Screen::Login(_)));

        harness.login("jan.novak").await;
        assert_eq!(harness.menu().draft().changes(), changes);
        assert!(harness.toast().contains("kept"), "{}", harness.toast());
    }

    #[tokio::test]
    async fn expired_session_changes_are_not_given_to_another_account() {
        let mut harness = Harness::logged_in("expired-other").await;
        harness.pick_lunch();

        mock::expire_session(&harness.server).await;
        harness.press(KeyCode::Char('s'));
        harness.settle().await;
        assert!(matches!(harness.app.screen, Screen::Login(_)));

        harness.login("eva.novakova").await;
        assert!(harness.menu().draft().is_empty());
    }

    #[tokio::test]
    async fn ctrl_c_asks_before_dropping_pending_changes() {
        let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        let mut harness = Harness::logged_in("ctrl-c-pending").await;
        harness.pick_lunch();

        harness.app.on_key(ctrl_c);
        assert!(harness.app.running);
        assert_eq!(harness.app.overlay, Overlay::ConfirmQuit);

        harness.app.on_key(ctrl_c);
        assert!(!harness.app.running);
    }
}
