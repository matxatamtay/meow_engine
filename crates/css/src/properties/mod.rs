mod model;
mod parser;
mod values;

pub use model::{
    ALL_PROPERTIES, CssWideKeyword, PropertyDeclaration, PropertyId, SpecifiedValue,
    W11_SNAPSHOT_PROPERTIES, W12_SNAPSHOT_PROPERTIES,
};
pub use parser::{parse_css_wide_keyword, parse_property_declaration, parse_property_declarations};
pub use values::{
    BorderWidthValue, BoxSizingValue, CSS_NUMBER_SCALE, ColorValue, ComputedValue, CssNumber,
    DisplayValue, Length, LengthOrAuto, LengthOrNone, LengthUnit, NamedColor, parse_computed_value,
};

#[cfg(test)]
mod tests;
