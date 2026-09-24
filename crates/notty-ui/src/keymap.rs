#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAction {
    MoveLeft, MoveRight, MoveUp, MoveDown, MoveHome, MoveEnd, MoveDocStart, MoveDocEnd,
    ExtendLeft, ExtendRight, ExtendUp, ExtendDown, ExtendHome, ExtendEnd,
    Backspace, DeleteForward, InsertNewline,
    Undo, Redo, SelectAll, Copy, Cut, Paste, Save, Find,
    None,
}

/// Traduce una tecla virtual de Win32 (`WM_KEYDOWN`'s `wparam`) + modificadores a una acción.
/// Los códigos son los estándar de `windows::Win32::UI::Input::KeyboardAndMouse` (VK_*).
pub fn action_for_vk(vk: u32, m: Modifiers) -> EditorAction {
    use EditorAction::*;
    match (vk, m.ctrl, m.shift) {
        (0x25, false, false) => MoveLeft,
        (0x25, false, true) => ExtendLeft,
        (0x27, false, false) => MoveRight,
        (0x27, false, true) => ExtendRight,
        (0x26, false, false) => MoveUp,
        (0x26, false, true) => ExtendUp,
        (0x28, false, false) => MoveDown,
        (0x28, false, true) => ExtendDown,
        (0x24, false, false) => MoveHome,
        (0x24, false, true) => ExtendHome,
        (0x24, true, _) => MoveDocStart,
        (0x23, false, false) => MoveEnd,
        (0x23, false, true) => ExtendEnd,
        (0x23, true, _) => MoveDocEnd,
        (0x08, _, _) => Backspace,
        (0x2E, _, _) => DeleteForward,
        (0x0D, _, _) => InsertNewline,
        (0x5A, true, false) => Undo,
        (0x5A, true, true) => Redo,
        (0x59, true, _) => Redo,
        (0x41, true, _) => SelectAll,
        (0x43, true, _) => Copy,
        (0x58, true, _) => Cut,
        (0x56, true, _) => Paste,
        (0x53, true, _) => Save,
        (0x46, true, _) => Find,
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(ctrl: bool, shift: bool) -> Modifiers {
        Modifiers { ctrl, shift, alt: false }
    }

    #[test]
    fn plain_arrows_move() {
        assert_eq!(action_for_vk(0x25, m(false, false)), EditorAction::MoveLeft);
        assert_eq!(action_for_vk(0x27, m(false, false)), EditorAction::MoveRight);
        assert_eq!(action_for_vk(0x26, m(false, false)), EditorAction::MoveUp);
        assert_eq!(action_for_vk(0x28, m(false, false)), EditorAction::MoveDown);
    }

    #[test]
    fn shift_arrows_extend_selection() {
        assert_eq!(action_for_vk(0x25, m(false, true)), EditorAction::ExtendLeft);
        assert_eq!(action_for_vk(0x28, m(false, true)), EditorAction::ExtendDown);
    }

    #[test]
    fn home_end_plain_and_shift() {
        assert_eq!(action_for_vk(0x24, m(false, false)), EditorAction::MoveHome);
        assert_eq!(action_for_vk(0x24, m(false, true)), EditorAction::ExtendHome);
        assert_eq!(action_for_vk(0x23, m(false, false)), EditorAction::MoveEnd);
    }

    #[test]
    fn ctrl_home_end_go_to_doc_bounds() {
        assert_eq!(action_for_vk(0x24, m(true, false)), EditorAction::MoveDocStart);
        assert_eq!(action_for_vk(0x23, m(true, false)), EditorAction::MoveDocEnd);
    }

    #[test]
    fn backspace_delete_enter() {
        assert_eq!(action_for_vk(0x08, m(false, false)), EditorAction::Backspace);
        assert_eq!(action_for_vk(0x2E, m(false, false)), EditorAction::DeleteForward);
        assert_eq!(action_for_vk(0x0D, m(false, false)), EditorAction::InsertNewline);
    }

    #[test]
    fn ctrl_letters_map_to_commands() {
        assert_eq!(action_for_vk(0x5A, m(true, false)), EditorAction::Undo);
        assert_eq!(action_for_vk(0x5A, m(true, true)), EditorAction::Redo);
        assert_eq!(action_for_vk(0x59, m(true, false)), EditorAction::Redo);
        assert_eq!(action_for_vk(0x41, m(true, false)), EditorAction::SelectAll);
        assert_eq!(action_for_vk(0x43, m(true, false)), EditorAction::Copy);
        assert_eq!(action_for_vk(0x58, m(true, false)), EditorAction::Cut);
        assert_eq!(action_for_vk(0x56, m(true, false)), EditorAction::Paste);
        assert_eq!(action_for_vk(0x53, m(true, false)), EditorAction::Save);
        assert_eq!(action_for_vk(0x46, m(true, false)), EditorAction::Find);
    }

    #[test]
    fn plain_letter_is_not_a_command() {
        assert_eq!(action_for_vk(0x41, m(false, false)), EditorAction::None);
    }

    #[test]
    fn unknown_key_is_none() {
        assert_eq!(action_for_vk(0x90, m(false, false)), EditorAction::None);
    }
}
