use shipyard::Component;

#[derive(Clone, Copy, Debug, Component)]
pub struct ParentRef(pub usize);
