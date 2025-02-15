use std::borrow::Cow;

use crate::app::models::{PlaylistDescription, PlaylistSummary};
use crate::app::state::{
    browser_state::{BrowserAction, BrowserEvent, BrowserState},
    login_state::{LoginAction, LoginEvent, LoginState},
    playback_state::{PlaybackAction, PlaybackEvent, PlaybackState},
    selection_state::{SelectionAction, SelectionContext, SelectionEvent, SelectionState},
    settings_state::{SettingsAction, SettingsEvent, SettingsState},
    ScreenName, UpdatableState,
};

// It's a big one...
// All possible actions!
// It's probably a VERY poor way to layout such a big enum, just look at the size, I'm so sorry I am not a sytems programmer
// Could use a few more Boxes maybe?
#[derive(Clone, Debug)]
pub enum AppAction {
    // With sub categories :)
    PlaybackAction(PlaybackAction),
    BrowserAction(BrowserAction),
    SelectionAction(SelectionAction),
    LoginAction(LoginAction),
    SettingsAction(SettingsAction),
    Start,
    Raise,
    ShowNotification(String),
    ViewNowPlaying,
    // Cross-state actions
    QueueSelection,
    DequeueSelection,
    MoveUpSelection,
    MoveDownSelection,
    SaveSelection,
    UnsaveSelection,
    EnableSelection(SelectionContext),
    CancelSelection,
    CreatePlaylist(PlaylistDescription),
    UpdatePlaylistName(PlaylistSummary),
}

// Not actual actions, just neat wrappers
impl AppAction {
    // An action to open a Spotify URI
    #[allow(non_snake_case)]
    pub fn OpenURI(uri: String) -> Option<Self> {
        debug!("parsing {}", &uri);
        let mut parts = uri.split(':');
        if parts.next()? != "spotify" {
            return None;
        }

        // Might start with /// because of https://gitlab.gnome.org/GNOME/glib/-/issues/1886/
        let action = parts
            .next()?
            .strip_prefix("///")
            .filter(|p| !p.is_empty())?;
        let data = parts.next()?;

        match action {
            "album" => Some(Self::ViewAlbum(data.to_string())),
            "artist" => Some(Self::ViewArtist(data.to_string())),
            "playlist" => Some(Self::ViewPlaylist(data.to_string())),
            "user" => Some(Self::ViewUser(data.to_string())),
            _ => None,
        }
    }

    #[allow(non_snake_case)]
    pub fn ViewAlbum(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::AlbumDetails(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewArtist(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::Artist(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewPlaylist(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::PlaylistDetails(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewUser(id: String) -> Self {
        BrowserAction::NavigationPush(ScreenName::User(id)).into()
    }

    #[allow(non_snake_case)]
    pub fn ViewSearch() -> Self {
        BrowserAction::NavigationPush(ScreenName::Search).into()
    }
}

// Actions mutate stuff, and we know what changed thanks to these events
#[derive(Clone, Debug)]
pub enum AppEvent {
    // Also subcategorized
    PlaybackEvent(PlaybackEvent),
    BrowserEvent(BrowserEvent),
    SelectionEvent(SelectionEvent),
    LoginEvent(LoginEvent),
    Started,
    Raised,
    NotificationShown(String),
    PlaylistCreatedNotificationShown(String),
    NowPlayingShown,
    SettingsEvent(SettingsEvent),
}

// The actual state, split five-ways
pub struct AppState {
    started: bool,
    pub playback: PlaybackState,
    pub browser: BrowserState,
    pub selection: SelectionState,
    pub logged_user: LoginState,
    pub settings: SettingsState,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            started: false,
            playback: Default::default(),
            browser: BrowserState::new(),
            selection: Default::default(),
            logged_user: Default::default(),
            settings: Default::default(),
        }
    }

    pub fn update_state(&mut self, message: AppAction) -> Vec<AppEvent> {
        match message {
            AppAction::Start if !self.started => {
                self.started = true;
                vec![AppEvent::Started]
            }
            // Couple of actions that don't mutate the state (not intested in keeping track of what they change)
            // they're here just to have a consistent way of doing things (always an Action)
            AppAction::ShowNotification(c) => vec![AppEvent::NotificationShown(c)],
            AppAction::ViewNowPlaying => vec![AppEvent::NowPlayingShown],
            AppAction::Raise => vec![AppEvent::Raised],
            // Cross-state actions: multiple "substates" are affected by these actions, that's why they're handled here
            // Might need some clean-up
            AppAction::QueueSelection => {
                self.playback.queue(self.selection.take_selection());
                vec![
                    SelectionEvent::SelectionModeChanged(false).into(),
                    PlaybackEvent::PlaylistChanged.into(),
                ]
            }
            AppAction::DequeueSelection => {
                let tracks: Vec<String> = self
                    .selection
                    .take_selection()
                    .into_iter()
                    .map(|s| s.id)
                    .collect();
                self.playback.dequeue(&tracks);

                vec![
                    SelectionEvent::SelectionModeChanged(false).into(),
                    PlaybackEvent::PlaylistChanged.into(),
                ]
            }
            AppAction::MoveDownSelection => {
                let mut selection = self.selection.peek_selection();
                let playback = &mut self.playback;
                selection
                    .next()
                    .and_then(|song| playback.move_down(&song.id))
                    .map(|_| vec![PlaybackEvent::PlaylistChanged.into()])
                    .unwrap_or_else(Vec::new)
            }
            AppAction::MoveUpSelection => {
                let mut selection = self.selection.peek_selection();
                let playback = &mut self.playback;
                selection
                    .next()
                    .and_then(|song| playback.move_up(&song.id))
                    .map(|_| vec![PlaybackEvent::PlaylistChanged.into()])
                    .unwrap_or_else(Vec::new)
            }
            AppAction::SaveSelection => {
                let tracks = self.selection.take_selection();
                let mut events: Vec<AppEvent> = forward_action(
                    BrowserAction::SaveTracks(tracks),
                    self.browser.home_state_mut().unwrap(),
                );
                events.push(SelectionEvent::SelectionModeChanged(false).into());
                events
            }
            AppAction::UnsaveSelection => {
                let tracks: Vec<String> = self
                    .selection
                    .take_selection()
                    .into_iter()
                    .map(|s| s.id)
                    .collect();
                let mut events: Vec<AppEvent> = forward_action(
                    BrowserAction::RemoveSavedTracks(tracks),
                    self.browser.home_state_mut().unwrap(),
                );
                events.push(SelectionEvent::SelectionModeChanged(false).into());
                events
            }
            AppAction::EnableSelection(context) => {
                if let Some(active) = self.selection.set_mode(Some(context)) {
                    vec![SelectionEvent::SelectionModeChanged(active).into()]
                } else {
                    vec![]
                }
            }
            AppAction::CancelSelection => {
                if let Some(active) = self.selection.set_mode(None) {
                    vec![SelectionEvent::SelectionModeChanged(active).into()]
                } else {
                    vec![]
                }
            }
            AppAction::CreatePlaylist(playlist) => {
                let id = playlist.id.clone();
                let mut events = forward_action(
                    LoginAction::PrependUserPlaylist(vec![playlist.clone().into()]),
                    &mut self.logged_user,
                );
                let mut more_events = forward_action(
                    BrowserAction::PrependPlaylistsContent(vec![playlist]),
                    &mut self.browser,
                );
                events.append(&mut more_events);
                events.push(AppEvent::PlaylistCreatedNotificationShown(id));
                events
            }
            AppAction::UpdatePlaylistName(s) => {
                let mut events = forward_action(
                    LoginAction::UpdateUserPlaylist(s.clone()),
                    &mut self.logged_user,
                );
                let mut more_events =
                    forward_action(BrowserAction::UpdatePlaylistName(s), &mut self.browser);
                events.append(&mut more_events);
                events
            }
            // As for all other actions, we forward them to the substates :)
            AppAction::PlaybackAction(a) => forward_action(a, &mut self.playback),
            AppAction::BrowserAction(a) => forward_action(a, &mut self.browser),
            AppAction::SelectionAction(a) => forward_action(a, &mut self.selection),
            AppAction::LoginAction(a) => forward_action(a, &mut self.logged_user),
            AppAction::SettingsAction(a) => forward_action(a, &mut self.settings),
            _ => vec![],
        }
    }
}

fn forward_action<A, E>(
    action: A,
    target: &mut impl UpdatableState<Action = A, Event = E>,
) -> Vec<AppEvent>
where
    A: Clone,
    E: Into<AppEvent>,
{
    target
        .update_with(Cow::Owned(action))
        .into_iter()
        .map(|e| e.into())
        .collect()
}
