use super::*;
use crate::command_menu::MenuShortcut;

/// Identifies keys the native menu consumes before egui receives them.
pub(super) fn native_accelerator_matches(
    id: &str,
    key: egui::Key,
    modifiers: egui::Modifiers,
    is_mac: bool,
) -> bool {
    let Some(action) = crate::command_menu::action(id) else {
        return false;
    };
    if action.shortcut == MenuShortcut::PasteSpecial {
        return key == egui::Key::V
            && !modifiers.shift
            && if is_mac {
                modifiers.mac_cmd && modifiers.ctrl && !modifiers.alt
            } else {
                modifiers.ctrl && modifiers.alt && !modifiers.mac_cmd
            };
    }
    let (expected_key, shift) = match action.shortcut {
        MenuShortcut::Command(character) => {
            let Some(key) = key_for_character(character) else {
                return false;
            };
            (key, false)
        }
        MenuShortcut::CommandShift(character) => {
            let Some(key) = key_for_character(character) else {
                return false;
            };
            (key, true)
        }
        MenuShortcut::Replace => (egui::Key::H, is_mac),
        MenuShortcut::Redo if !is_mac => (egui::Key::Y, false),
        MenuShortcut::Redo => (egui::Key::Z, true),
        MenuShortcut::ZoomIn => (egui::Key::Plus, true),
        MenuShortcut::Settings if is_mac => (egui::Key::Comma, false),
        MenuShortcut::None | MenuShortcut::Settings | MenuShortcut::PasteSpecial => return false,
    };
    key == expected_key
        && modifiers.command
        && modifiers.shift == shift
        && !modifiers.alt
        && if is_mac {
            modifiers.mac_cmd && !modifiers.ctrl
        } else {
            modifiers.ctrl && !modifiers.mac_cmd
        }
}

/// Maps menu characters to egui's logical keys.
fn key_for_character(character: char) -> Option<egui::Key> {
    match character {
        'N' => Some(egui::Key::N),
        'O' => Some(egui::Key::O),
        'S' => Some(egui::Key::S),
        'B' => Some(egui::Key::B),
        'I' => Some(egui::Key::I),
        'E' => Some(egui::Key::E),
        'L' => Some(egui::Key::L),
        'Z' => Some(egui::Key::Z),
        'W' => Some(egui::Key::W),
        'D' => Some(egui::Key::D),
        'R' => Some(egui::Key::R),
        'X' => Some(egui::Key::X),
        'C' => Some(egui::Key::C),
        'V' => Some(egui::Key::V),
        'A' => Some(egui::Key::A),
        'F' => Some(egui::Key::F),
        '-' => Some(egui::Key::Minus),
        '0' => Some(egui::Key::Num0),
        _ => None,
    }
}
