//! The sticks of a profile. The controls in one group are one object on the
//! pad, and the profile gives that object its name.

use crate::model::{Control, ControllerProfile, StickDirection};
use crate::Diagnostic;
use std::collections::BTreeMap;

/// Add every problem with the sticks of `profile` to `problems`: a direction
/// on a control outside a stick, a member without a direction or out of
/// capture order, a stick without a title, and a title for no stick.
pub(crate) fn check(
    id: &str,
    package: &str,
    profile: &ControllerProfile,
    problems: &mut Vec<Diagnostic>,
) {
    let mut sticks: BTreeMap<&str, Vec<&Control>> = BTreeMap::new();
    for control in &profile.controls {
        match &control.group {
            Some(group) => sticks.entry(group.as_str()).or_default().push(control),
            None if control.direction.is_some() => problems.push(Diagnostic::new(
                "control.direction_outside_stick",
                package,
                format!("{id}.controls.{}", control.id),
                "only a member of a stick has a direction",
            )),
            None => {}
        }
    }
    for (stick, members) in &sticks {
        let titled = profile.groups.get(*stick).map(|group| group.title.trim());
        if titled.is_none_or(str::is_empty) {
            problems.push(Diagnostic::new(
                "controller.group_untitled",
                package,
                format!("{id}.groups.{stick}"),
                "every stick has a title: what the pad calls it",
            ));
        }
        // Each member points one way, once, in the order in which we capture
        // them in the menu.
        let directions: Vec<StickDirection> =
            members.iter().filter_map(|member| member.direction).collect();
        if directions.len() != members.len() {
            problems.push(Diagnostic::new(
                "control.stick_direction_missing",
                package,
                format!("{id}.controls[group={stick}]"),
                "every member of a stick has a direction: up, right, down, left or press",
            ));
        } else if directions.windows(2).any(|pair| pair[0] >= pair[1]) {
            problems.push(Diagnostic::new(
                "control.stick_direction_order",
                package,
                format!("{id}.controls[group={stick}]"),
                "a stick declares each direction once, in the order up, right, down, left, press",
            ));
        }
    }
    for group in profile.groups.keys() {
        if !sticks.contains_key(group.as_str()) {
            problems.push(Diagnostic::new(
                "controller.group_unused",
                package,
                format!("{id}.groups.{group}"),
                "no control is in this group",
            ));
        }
    }
}
