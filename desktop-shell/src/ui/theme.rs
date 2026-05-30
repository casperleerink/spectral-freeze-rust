use nih_plug_egui::egui::Color32;

pub(super) struct UiTheme {
    pub bg: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub border: Color32,
    pub accent: Color32,
    pub fg: Color32,
    pub fg_dim: Color32,
}

impl Default for UiTheme {
    fn default() -> Self {
        Self {
            bg: Color32::from_rgb(15, 15, 20),
            panel: Color32::from_rgb(25, 25, 34),
            panel2: Color32::from_rgb(35, 35, 46),
            border: Color32::from_rgb(56, 56, 72),
            accent: Color32::from_rgb(124, 196, 255),
            fg: Color32::from_rgb(232, 232, 240),
            fg_dim: Color32::from_rgb(146, 146, 164),
        }
    }
}
