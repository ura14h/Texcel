use super::*;

#[test]
// Creates one new window at a time, so each native window can be matched to its viewport,
// and lets a waiting viewport in once the window before it is identified.
fn windows_are_created_one_at_a_time() {
    let first = egui::ViewportId::from_hash_of("first");
    let second = egui::ViewportId::from_hash_of("second");
    let mut creation = WindowCreation::default();
    assert_eq!(creation.admit(first, false), Admission::Create);
    assert_eq!(creation.admit(second, false), Admission::Wait);
    // While the first window is not identified, it keeps registering and the second waits.
    assert_eq!(creation.admit(first, false), Admission::Registered);
    assert_eq!(creation.admit(second, false), Admission::Wait);
    // Once identified, the second window is created.
    assert_eq!(creation.admit(second, true), Admission::Create);
    assert_eq!(creation.admit(first, false), Admission::Registered);
}

#[test]
// Frees the creation slot when the viewport being created is no longer wanted.
fn a_dropped_viewport_frees_the_creation_slot() {
    let help = egui::ViewportId::from_hash_of("help");
    let document = egui::ViewportId::from_hash_of("document");
    let mut creation = WindowCreation::default();
    assert_eq!(creation.admit(help, false), Admission::Create);
    assert!(creation.forget_unwanted(&BTreeSet::from([document])));
    assert_eq!(creation.admit(document, false), Admission::Create);
}
