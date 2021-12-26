use bevy::{prelude::Handle, text::Font};

/// Resource that stores fonts that can be used in the application.
#[derive(Debug, Default)]
pub struct Fonts {
    pub bold: Handle<Font>,
    pub regular: Handle<Font>,
}
