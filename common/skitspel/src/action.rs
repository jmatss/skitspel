/// Contains the current action performed by a specific player.
#[derive(Debug, Clone, Default)]
pub struct PlayerAction {
    pub up_pressed: bool,
    pub right_pressed: bool,
    pub down_pressed: bool,
    pub left_pressed: bool,
    pub a_pressed: bool,
    pub b_pressed: bool,

    /// Contains the previous/latest action that was performed on this `PlayerAction`.
    pub prev_action: ActionEvent,

    /// If this is set to true, there have been a new actions since we checked
    /// the current value(s) of this `PlayerAction`.
    pub new_action_since_last_read: bool,
}

/// An event that represents the action sent from a specfic client.
#[derive(Clone, Copy, Eq, PartialEq, Debug, Hash)]
pub enum ActionEvent {
    UpPressed,
    UpReleased,
    RightPressed,
    RightReleased,
    DownPressed,
    DownReleased,
    LeftPressed,
    LeftReleased,
    APressed,
    AReleased,
    BPressed,
    BReleased,
    None,
}

impl Default for ActionEvent {
    fn default() -> Self {
        ActionEvent::None
    }
}
