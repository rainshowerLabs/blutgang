// Recommended way to deal with this, idk either
pub type Error = Box<dyn std::error::Error + Send + Sync + 'static>;

// TODO: Placeholder, implement propper errors
