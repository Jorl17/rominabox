//! The grammar of the menu script in `menu/drivers/rmlui/script_commands.inc`:
//! the steps that are commands in a test build of the player, neither
//! `name:value` nor an element to click.

const SOURCE: &str =
    include_str!("../../../../vendor/retroarch/menu/drivers/rmlui/script_commands.inc");

/// Each command step, as a script spells it.
pub fn commands() -> Vec<&'static str> {
    let commands: Vec<&'static str> = super::inc::declarations(SOURCE)
        .filter(|declaration| declaration.macro_name() == "RIB_SCRIPT_COMMAND")
        .map(|declaration| {
            declaration
                .field(1)
                .expect("script_commands.inc: RIB_SCRIPT_COMMAND(Name, \"step\")")
        })
        .collect();
    assert!(!commands.is_empty(), "script_commands.inc declares no commands");
    commands
}
