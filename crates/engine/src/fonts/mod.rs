mod database;
mod model;

pub use database::{FontDatabase, script_for};
pub use model::{
    FontCoverage, FontFace, FontId, FontRequest, FontSlant, FontSource, FontSpan, Script,
};

#[cfg(test)]
mod tests;
