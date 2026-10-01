//! Application menu composition shared by the workspace and fixture adapters.
//! Presentation stays in menu; availability and effects stay in each host.
use crate::input::actions::ActionId;

pub(crate) const FILE_ACTIONS: &[ActionId] = &[
    ActionId::New,
    ActionId::Open,
    ActionId::Import,
    ActionId::Save,
    ActionId::SaveAs,
];
pub(crate) const CONTEXT_EDIT_ACTIONS: &[ActionId] = &[
    ActionId::SelectAll,
    ActionId::Duplicate,
    ActionId::MakeFace,
    ActionId::Delete,
];
pub(crate) const CONTEXT_VIEW_ACTIONS: &[ActionId] = &[
    ActionId::FrameAll,
    ActionId::FrameSelection,
    ActionId::Preferences,
];
pub(crate) const VIEW_TOGGLE_ACTIONS: &[ActionId] =
    &[ActionId::Edges, ActionId::Xray, ActionId::Ruler2D];
