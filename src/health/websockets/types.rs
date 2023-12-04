#[derive(Debug, Copy, Clone)]
pub enum CreateRemoveWs {
    Create,
    Remove,
}

#[derive(Debug, Copy, Clone)]
pub struct Wsreg {
    pub create_remove: CreateRemoveWs,
    pub id: Option<usize>,
}
