fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Resolution is the host's, shared with the desktop, so both surfaces read
    // one credential store and a login in either is a login in both.
    let home = optimus_host::resolve_home(std::env::args().nth(1).as_deref());
    optimus_tui::run(home)
}
