//! Non-raster component edits preserve authored control points and their combination order.
use super::{invalid, Action, Input};
use aura_core::AuraResult;
use aura_recipe::local_masks::{LocalMask, Mode, Source};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Transform {
    pub offset: [f32; 2],
    pub scale: [f32; 2],
    pub angle: f32,
}

pub(super) fn is_authored(action: Action) -> bool {
    matches!(
        action,
        Action::EditLinear
            | Action::EditRadial
            | Action::EditBrush
            | Action::EditLuminance
            | Action::ComponentMode
            | Action::ReorderComponent
            | Action::ExtractComponent
            | Action::CopyComponent
    )
}

fn same_geometry(action: Action, current: &Source, replacement: &Source) -> bool {
    matches!(
        (action, current, replacement),
        (
            Action::EditLinear,
            Source::Linear { .. },
            Source::Linear { .. }
        ) | (
            Action::EditRadial,
            Source::Radial { .. },
            Source::Radial { .. }
        ) | (
            Action::EditBrush,
            Source::Brush { .. },
            Source::Brush { .. }
        ) | (
            Action::EditLuminance,
            Source::Luminance { .. },
            Source::Luminance { .. }
        )
    )
}

pub(super) fn edit(input: &Input, masks: &mut Vec<LocalMask>) -> AuraResult<String> {
    let index = masks
        .iter()
        .position(|m| m.id == input.mask_id)
        .ok_or_else(|| invalid("That mask no longer exists."))?;
    let original = masks
        .get(index)
        .cloned()
        .ok_or_else(|| invalid("That mask no longer exists."))?;
    let mut component = original
        .components
        .get(input.component_index)
        .cloned()
        .ok_or_else(|| invalid("Choose an existing mask component."))?;
    let mut changed = original.clone();
    match input.action {
        Action::EditLinear | Action::EditRadial | Action::EditBrush | Action::EditLuminance => {
            let source = input
                .source
                .clone()
                .ok_or_else(|| invalid("Enter component geometry."))?;
            let valid = same_geometry(input.action, &component.source, &source);
            if !valid {
                return Err(invalid("Choose a component of the matching geometry type."));
            }
            component.source = source;
            let destination = changed
                .components
                .get_mut(input.component_index)
                .ok_or_else(|| invalid("Missing component."))?;
            *destination = component;
        }
        Action::ComponentMode => {
            component.mode = input
                .mode
                .ok_or_else(|| invalid("Choose a combination mode."))?;
            let destination = changed
                .components
                .get_mut(input.component_index)
                .ok_or_else(|| invalid("Missing component."))?;
            *destination = component;
        }
        Action::ReorderComponent => {
            if input.target_index >= changed.components.len() {
                return Err(invalid("Choose an existing destination position."));
            }
            changed.components.remove(input.component_index);
            changed.components.insert(input.target_index, component);
        }
        Action::ExtractComponent => {
            if masks.len() >= aura_recipe::local_masks::MAX_MASKS {
                return Err(invalid("Maximum mask count reached."));
            }
            // Extraction makes an independent selection; its previous combination has no predecessor.
            component.mode = Mode::Add;
            changed.id = format!("mask-{}", uuid::Uuid::new_v4().simple());
            changed.name = format!(
                "{} part",
                original.name.chars().take(20).collect::<String>()
            );
            changed.components = vec![component];
            let id = changed.id.clone();
            masks.push(changed);
            return Ok(id);
        }
        Action::CopyComponent => {
            let target = masks
                .iter_mut()
                .find(|mask| Some(&mask.id) == input.other_mask_id.as_ref())
                .ok_or_else(|| invalid("Choose a destination mask."))?;
            if target.components.len() >= aura_recipe::local_masks::MAX_COMPONENTS {
                return Err(invalid("Maximum component count reached."));
            }
            target.components.push(component);
            return Ok(target.id.clone());
        }
        _ => return Err(invalid("Invalid authored selection operation.")),
    }
    let destination = masks
        .get_mut(index)
        .ok_or_else(|| invalid("Missing mask."))?;
    *destination = changed;
    Ok(input.mask_id.clone())
}
