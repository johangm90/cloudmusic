use libadwaita as adw;

/// Builds the application header bar with view switcher
pub fn build_header(stack: &adw::ViewStack) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();

    let switcher = adw::ViewSwitcher::new();
    switcher.set_stack(Some(stack));
    switcher.set_policy(adw::ViewSwitcherPolicy::Wide);

    header.set_title_widget(Some(&switcher));
    header
}
