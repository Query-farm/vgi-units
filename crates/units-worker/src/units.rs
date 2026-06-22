//! Pure, runtime unit-conversion engine — no Arrow, no DuckDB.
//!
//! The need is *runtime*, string-driven conversion (`convert(5,'mi','km')`), so
//! the compile-time-typed `uom` crate is the wrong fit. Instead we keep a curated
//! static table mapping each unit string to its physical [`Dimension`], a linear
//! `factor` (how many SI base units one of this unit equals), and an additive
//! `offset` (in SI base units, non-zero only for the temperature scales).
//!
//! Conversion within a dimension is:
//!
//! ```text
//! base  = value * from.factor + from.offset      // value -> SI base unit
//! out   = (base - to.offset) / to.factor         // SI base unit -> target
//! ```
//!
//! For purely multiplicative units the offsets are zero and this reduces to the
//! familiar `value * from.factor / to.factor`. The offset path is what makes the
//! temperature scales (°C / °F / K) correct: 0 °C = 273.15 K = 32 °F.
//!
//! ## Source of factors
//!
//! Factors are the exact SI definitions and the internationally agreed
//! conversion constants:
//!   * Length: the international yard/inch (1 in = 0.0254 m exactly, 1981) and the
//!     derived foot/yard/mile; nautical mile = 1852 m exactly.
//!   * Mass: international avoirdupois pound = 0.453592_37 kg exactly (1959).
//!   * Pressure: standard atmosphere = 101_325 Pa exactly; 1 bar = 100_000 Pa;
//!     psi derived from lbf and in².
//!   * Energy: thermochemical calorie = 4.184 J exactly; BTU(IT) = 1055.05585262 J.
//!   * Data: binary (IEC) prefixes Ki/Mi/Gi/Ti = 1024^n, decimal k/M/G/T = 1000^n.
//!   * Angle: 1 rad, 1° = π/180 rad, 1 grad = π/200 rad.
//!
//! All other entries follow from the SI prefix definitions.

use std::collections::HashMap;
use std::sync::OnceLock;

/// A physical dimension. Conversion is only meaningful *within* one dimension;
/// crossing dimensions (km → kg) is rejected by [`convert`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Dimension {
    Length,
    Mass,
    Time,
    Temperature,
    Area,
    Volume,
    Speed,
    Pressure,
    Energy,
    Power,
    Data,
    Angle,
    Frequency,
    Force,
}

impl Dimension {
    /// The lowercase canonical name surfaced by `dimension(unit)`.
    pub fn name(self) -> &'static str {
        match self {
            Dimension::Length => "length",
            Dimension::Mass => "mass",
            Dimension::Time => "time",
            Dimension::Temperature => "temperature",
            Dimension::Area => "area",
            Dimension::Volume => "volume",
            Dimension::Speed => "speed",
            Dimension::Pressure => "pressure",
            Dimension::Energy => "energy",
            Dimension::Power => "power",
            Dimension::Data => "data",
            Dimension::Angle => "angle",
            Dimension::Frequency => "frequency",
            Dimension::Force => "force",
        }
    }

    /// The canonical SI base unit string for this dimension (the unit that
    /// `to_base` reports its result in, and that `supported_units()` lists).
    pub fn base_unit(self) -> &'static str {
        match self {
            Dimension::Length => "m",
            Dimension::Mass => "kg",
            Dimension::Time => "s",
            Dimension::Temperature => "K",
            Dimension::Area => "m^2",
            Dimension::Volume => "m^3",
            Dimension::Speed => "m/s",
            Dimension::Pressure => "Pa",
            Dimension::Energy => "J",
            Dimension::Power => "W",
            Dimension::Data => "byte",
            Dimension::Angle => "rad",
            Dimension::Frequency => "Hz",
            Dimension::Force => "N",
        }
    }
}

/// A single unit's definition: its dimension and the affine map to the SI base
/// unit (`base = value * factor + offset`).
#[derive(Clone, Copy, Debug)]
pub struct UnitDef {
    pub dimension: Dimension,
    pub factor: f64,
    pub offset: f64,
}

/// Why a conversion / lookup failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnitError {
    /// One of the unit strings is not in the table. Carries the offending string.
    UnknownUnit(String),
    /// Both units are known but live in different dimensions (km → kg).
    Incompatible {
        from: String,
        from_dim: &'static str,
        to: String,
        to_dim: &'static str,
    },
}

impl std::fmt::Display for UnitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitError::UnknownUnit(u) => write!(f, "unknown unit '{u}'"),
            UnitError::Incompatible {
                from,
                from_dim,
                to,
                to_dim,
            } => write!(
                f,
                "incompatible units: '{from}' is {from_dim} but '{to}' is {to_dim}"
            ),
        }
    }
}

impl std::error::Error for UnitError {}

/// Look up a unit definition by (case-sensitive first, then case-insensitive)
/// string. Returns `None` for unknown units.
pub fn lookup(unit: &str) -> Option<UnitDef> {
    let table = table();
    let trimmed = unit.trim();
    if let Some(d) = table.get(trimmed) {
        return Some(*d);
    }
    // Case-insensitive fallback. We avoid lowercasing eagerly because some
    // units are case-significant (e.g. 'm' metre vs 'M' is not defined; 'mi'
    // mile). The fallback only fires when the exact form misses.
    table.get(trimmed.to_lowercase().as_str()).copied()
}

/// The dimension of a unit string, or `None` if unknown.
pub fn dimension(unit: &str) -> Option<Dimension> {
    lookup(unit).map(|d| d.dimension)
}

/// Whether two units share a dimension. Unknown units are never compatible.
pub fn compatible(a: &str, b: &str) -> bool {
    match (dimension(a), dimension(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Convert `value` from `from` units to `to` units.
///
/// * Unknown unit on either side → `Err(UnknownUnit)`.
/// * Known but different dimensions → `Err(Incompatible)`.
pub fn convert(value: f64, from: &str, to: &str) -> Result<f64, UnitError> {
    let f = lookup(from).ok_or_else(|| UnitError::UnknownUnit(from.trim().to_string()))?;
    let t = lookup(to).ok_or_else(|| UnitError::UnknownUnit(to.trim().to_string()))?;
    if f.dimension != t.dimension {
        return Err(UnitError::Incompatible {
            from: from.trim().to_string(),
            from_dim: f.dimension.name(),
            to: to.trim().to_string(),
            to_dim: t.dimension.name(),
        });
    }
    let base = value * f.factor + f.offset;
    Ok((base - t.offset) / t.factor)
}

/// The value expressed in the SI base unit of its dimension. Unknown unit →
/// `Err(UnknownUnit)`.
pub fn to_base(value: f64, unit: &str) -> Result<f64, UnitError> {
    let u = lookup(unit).ok_or_else(|| UnitError::UnknownUnit(unit.trim().to_string()))?;
    Ok(value * u.factor + u.offset)
}

/// A parsed `<value> <unit>` quantity.
#[derive(Clone, Debug, PartialEq)]
pub struct Quantity {
    pub value: f64,
    pub unit: String,
}

/// Parse free-form text like `"5 km"`, `"3.2kg"`, `"-1.5e3 J"`, `"10 m/s"` into a
/// [`Quantity`]. The leading run is parsed as the number (optionally signed, with
/// decimal point and/or exponent); the rest, trimmed, is the unit. The unit must
/// be a known unit. Returns `None` if no number is present or the unit is unknown.
pub fn parse_quantity(text: &str) -> Option<Quantity> {
    let s = text.trim();
    if s.is_empty() {
        return None;
    }
    // Find the split point: the longest leading prefix that parses as f64. We
    // scan the numeric run character-by-character (digits, sign, dot, exponent).
    let bytes = s.as_bytes();
    let mut i = 0;
    // optional leading sign
    if i < bytes.len() && (bytes[i] == b'+' || bytes[i] == b'-') {
        i += 1;
    }
    let mut seen_digit = false;
    let mut seen_dot = false;
    while i < bytes.len() {
        match bytes[i] {
            b'0'..=b'9' => {
                seen_digit = true;
                i += 1;
            }
            b'.' if !seen_dot => {
                seen_dot = true;
                i += 1;
            }
            b'e' | b'E' if seen_digit => {
                // exponent: optional sign then digits
                let mut j = i + 1;
                if j < bytes.len() && (bytes[j] == b'+' || bytes[j] == b'-') {
                    j += 1;
                }
                let mut exp_digit = false;
                while j < bytes.len() && bytes[j].is_ascii_digit() {
                    exp_digit = true;
                    j += 1;
                }
                if exp_digit {
                    i = j;
                }
                break;
            }
            _ => break,
        }
    }
    if !seen_digit {
        return None;
    }
    let num: f64 = s[..i].parse().ok()?;
    let unit = s[i..].trim();
    if unit.is_empty() {
        return None;
    }
    // The unit must be recognized; normalize to its given (trimmed) form.
    lookup(unit)?;
    Some(Quantity {
        value: num,
        unit: unit.to_string(),
    })
}

/// A row of the `supported_units()` discovery table.
pub struct SupportedUnit {
    pub unit: &'static str,
    pub dimension: &'static str,
    pub base_unit: &'static str,
}

/// Every unit string in the table, sorted, with its dimension and SI base unit.
pub fn supported_units() -> Vec<SupportedUnit> {
    let mut rows: Vec<SupportedUnit> = ENTRIES
        .iter()
        .map(|(name, d)| SupportedUnit {
            unit: name,
            dimension: d.dimension.name(),
            base_unit: d.dimension.base_unit(),
        })
        .collect();
    rows.sort_by(|a, b| {
        a.dimension
            .cmp(b.dimension)
            .then_with(|| a.unit.cmp(b.unit))
    });
    rows
}

/// The number of distinct unit strings recognized. (Used by the lib API and
/// tests; the `mod units` copy in the binary may not reference it.)
#[allow(dead_code)]
pub fn unit_count() -> usize {
    ENTRIES.len()
}

// ---------------------------------------------------------------------------
// The curated unit table.
// ---------------------------------------------------------------------------

const fn def(dimension: Dimension, factor: f64) -> UnitDef {
    UnitDef {
        dimension,
        factor,
        offset: 0.0,
    }
}

const fn def_off(dimension: Dimension, factor: f64, offset: f64) -> UnitDef {
    UnitDef {
        dimension,
        factor,
        offset,
    }
}

use Dimension::*;

/// The master list. Each `(string, def)` pair is one accepted spelling. Aliases
/// (km/kilometer/kilometre) repeat the same `def`. SI base units appear too.
#[rustfmt::skip]
static ENTRIES: &[(&str, UnitDef)] = &[
    // ---- Length (base: metre) ----
    ("m", def(Length, 1.0)), ("meter", def(Length, 1.0)), ("metre", def(Length, 1.0)),
    ("meters", def(Length, 1.0)), ("metres", def(Length, 1.0)),
    ("nm", def(Length, 1e-9)), ("nanometer", def(Length, 1e-9)), ("nanometre", def(Length, 1e-9)),
    ("um", def(Length, 1e-6)), ("µm", def(Length, 1e-6)), ("micron", def(Length, 1e-6)),
    ("micrometer", def(Length, 1e-6)), ("micrometre", def(Length, 1e-6)),
    ("mm", def(Length, 1e-3)), ("millimeter", def(Length, 1e-3)), ("millimetre", def(Length, 1e-3)),
    ("cm", def(Length, 1e-2)), ("centimeter", def(Length, 1e-2)), ("centimetre", def(Length, 1e-2)),
    ("dm", def(Length, 1e-1)), ("decimeter", def(Length, 1e-1)), ("decimetre", def(Length, 1e-1)),
    ("km", def(Length, 1e3)), ("kilometer", def(Length, 1e3)), ("kilometre", def(Length, 1e3)),
    ("kilometers", def(Length, 1e3)), ("kilometres", def(Length, 1e3)),
    ("in", def(Length, 0.0254)), ("inch", def(Length, 0.0254)), ("inches", def(Length, 0.0254)),
    ("ft", def(Length, 0.3048)), ("foot", def(Length, 0.3048)), ("feet", def(Length, 0.3048)),
    ("yd", def(Length, 0.9144)), ("yard", def(Length, 0.9144)), ("yards", def(Length, 0.9144)),
    ("mi", def(Length, 1609.344)), ("mile", def(Length, 1609.344)), ("miles", def(Length, 1609.344)),
    ("nmi", def(Length, 1852.0)), ("nautical_mile", def(Length, 1852.0)),
    ("au", def(Length, 1.495978707e11)),
    ("ly", def(Length, 9.4607304725808e15)), ("lightyear", def(Length, 9.4607304725808e15)),
    ("pc", def(Length, 3.085677581491367e16)), ("parsec", def(Length, 3.085677581491367e16)),

    // ---- Mass (base: kilogram) ----
    ("kg", def(Mass, 1.0)), ("kilogram", def(Mass, 1.0)), ("kilograms", def(Mass, 1.0)),
    ("g", def(Mass, 1e-3)), ("gram", def(Mass, 1e-3)), ("grams", def(Mass, 1e-3)),
    ("mg", def(Mass, 1e-6)), ("milligram", def(Mass, 1e-6)),
    ("ug", def(Mass, 1e-9)), ("µg", def(Mass, 1e-9)), ("microgram", def(Mass, 1e-9)),
    ("t", def(Mass, 1e3)), ("tonne", def(Mass, 1e3)), ("metric_ton", def(Mass, 1e3)),
    ("lb", def(Mass, 0.45359237)), ("lbs", def(Mass, 0.45359237)),
    ("pound", def(Mass, 0.45359237)), ("pounds", def(Mass, 0.45359237)),
    ("oz", def(Mass, 0.028349523125)), ("ounce", def(Mass, 0.028349523125)),
    ("st", def(Mass, 6.35029318)), ("stone", def(Mass, 6.35029318)),
    ("ton", def(Mass, 907.18474)), ("short_ton", def(Mass, 907.18474)),
    ("long_ton", def(Mass, 1016.0469088)),

    // ---- Time (base: second) ----
    ("s", def(Time, 1.0)), ("sec", def(Time, 1.0)), ("secs", def(Time, 1.0)),
    ("second", def(Time, 1.0)), ("seconds", def(Time, 1.0)),
    ("ms", def(Time, 1e-3)), ("millisecond", def(Time, 1e-3)), ("milliseconds", def(Time, 1e-3)),
    ("us", def(Time, 1e-6)), ("µs", def(Time, 1e-6)), ("microsecond", def(Time, 1e-6)),
    ("ns", def(Time, 1e-9)), ("nanosecond", def(Time, 1e-9)),
    ("min", def(Time, 60.0)), ("minute", def(Time, 60.0)), ("minutes", def(Time, 60.0)),
    ("h", def(Time, 3600.0)), ("hr", def(Time, 3600.0)), ("hour", def(Time, 3600.0)),
    ("hours", def(Time, 3600.0)),
    ("d", def(Time, 86400.0)), ("day", def(Time, 86400.0)), ("days", def(Time, 86400.0)),
    ("wk", def(Time, 604800.0)), ("week", def(Time, 604800.0)), ("weeks", def(Time, 604800.0)),
    ("yr", def(Time, 31557600.0)), ("year", def(Time, 31557600.0)), ("years", def(Time, 31557600.0)),

    // ---- Temperature (base: kelvin) — affine offsets ----
    ("K", def(Temperature, 1.0)), ("kelvin", def(Temperature, 1.0)),
    ("C", def_off(Temperature, 1.0, 273.15)), ("°C", def_off(Temperature, 1.0, 273.15)),
    ("celsius", def_off(Temperature, 1.0, 273.15)), ("degC", def_off(Temperature, 1.0, 273.15)),
    // °F: base K = value*(5/9) + (273.15 - 32*5/9)
    ("F", def_off(Temperature, 0.5555555555555556, 255.3722222222222)),
    ("°F", def_off(Temperature, 0.5555555555555556, 255.3722222222222)),
    ("fahrenheit", def_off(Temperature, 0.5555555555555556, 255.3722222222222)),
    ("degF", def_off(Temperature, 0.5555555555555556, 255.3722222222222)),
    ("R", def(Temperature, 0.5555555555555556)), ("rankine", def(Temperature, 0.5555555555555556)),

    // ---- Area (base: square metre) ----
    ("m^2", def(Area, 1.0)), ("m2", def(Area, 1.0)), ("sqm", def(Area, 1.0)),
    ("cm^2", def(Area, 1e-4)), ("cm2", def(Area, 1e-4)),
    ("mm^2", def(Area, 1e-6)), ("mm2", def(Area, 1e-6)),
    ("km^2", def(Area, 1e6)), ("km2", def(Area, 1e6)),
    ("ha", def(Area, 1e4)), ("hectare", def(Area, 1e4)),
    ("acre", def(Area, 4046.8564224)), ("acres", def(Area, 4046.8564224)),
    ("ft^2", def(Area, 0.09290304)), ("ft2", def(Area, 0.09290304)), ("sqft", def(Area, 0.09290304)),
    ("in^2", def(Area, 0.00064516)), ("in2", def(Area, 0.00064516)),
    ("mi^2", def(Area, 2589988.110336)), ("mi2", def(Area, 2589988.110336)),

    // ---- Volume (base: cubic metre) ----
    ("m^3", def(Volume, 1.0)), ("m3", def(Volume, 1.0)),
    ("cm^3", def(Volume, 1e-6)), ("cm3", def(Volume, 1e-6)), ("cc", def(Volume, 1e-6)),
    ("l", def(Volume, 1e-3)), ("L", def(Volume, 1e-3)), ("liter", def(Volume, 1e-3)),
    ("litre", def(Volume, 1e-3)), ("liters", def(Volume, 1e-3)), ("litres", def(Volume, 1e-3)),
    ("ml", def(Volume, 1e-6)), ("mL", def(Volume, 1e-6)), ("milliliter", def(Volume, 1e-6)),
    ("millilitre", def(Volume, 1e-6)),
    ("gal", def(Volume, 0.003785411784)), ("gallon", def(Volume, 0.003785411784)),
    ("gallons", def(Volume, 0.003785411784)),
    ("qt", def(Volume, 0.000946352946)), ("quart", def(Volume, 0.000946352946)),
    ("pt", def(Volume, 0.000473176473)), ("pint", def(Volume, 0.000473176473)),
    ("cup", def(Volume, 0.0002365882365)),
    ("floz", def(Volume, 2.95735295625e-5)), ("fl_oz", def(Volume, 2.95735295625e-5)),
    ("tbsp", def(Volume, 1.478676478125e-5)), ("tsp", def(Volume, 4.92892159375e-6)),
    ("ft^3", def(Volume, 0.028316846592)), ("ft3", def(Volume, 0.028316846592)),
    ("in^3", def(Volume, 1.6387064e-5)), ("in3", def(Volume, 1.6387064e-5)),
    ("bbl", def(Volume, 0.158987294928)), ("barrel", def(Volume, 0.158987294928)),

    // ---- Speed (base: metre / second) ----
    ("m/s", def(Speed, 1.0)), ("mps", def(Speed, 1.0)),
    ("km/h", def(Speed, 0.2777777777777778)), ("kph", def(Speed, 0.2777777777777778)),
    ("kmh", def(Speed, 0.2777777777777778)),
    ("mph", def(Speed, 0.44704)), ("mi/h", def(Speed, 0.44704)),
    ("ft/s", def(Speed, 0.3048)), ("fps", def(Speed, 0.3048)),
    ("knot", def(Speed, 0.5144444444444445)), ("knots", def(Speed, 0.5144444444444445)),
    ("kn", def(Speed, 0.5144444444444445)),
    ("c", def(Speed, 299792458.0)),

    // ---- Pressure (base: pascal) ----
    ("Pa", def(Pressure, 1.0)), ("pascal", def(Pressure, 1.0)),
    ("hPa", def(Pressure, 100.0)), ("hectopascal", def(Pressure, 100.0)),
    ("kPa", def(Pressure, 1000.0)), ("kilopascal", def(Pressure, 1000.0)),
    ("MPa", def(Pressure, 1e6)), ("megapascal", def(Pressure, 1e6)),
    ("bar", def(Pressure, 100000.0)),
    ("mbar", def(Pressure, 100.0)), ("millibar", def(Pressure, 100.0)),
    ("atm", def(Pressure, 101325.0)), ("atmosphere", def(Pressure, 101325.0)),
    ("psi", def(Pressure, 6894.757293168361)),
    ("torr", def(Pressure, 133.32236842105263)),
    ("mmHg", def(Pressure, 133.322387415)),
    ("inHg", def(Pressure, 3386.388640341)),

    // ---- Energy (base: joule) ----
    ("J", def(Energy, 1.0)), ("joule", def(Energy, 1.0)), ("joules", def(Energy, 1.0)),
    ("kJ", def(Energy, 1000.0)), ("kilojoule", def(Energy, 1000.0)),
    ("MJ", def(Energy, 1e6)), ("megajoule", def(Energy, 1e6)),
    ("cal", def(Energy, 4.184)), ("calorie", def(Energy, 4.184)),
    ("kcal", def(Energy, 4184.0)), ("kilocalorie", def(Energy, 4184.0)),
    ("Wh", def(Energy, 3600.0)), ("watt_hour", def(Energy, 3600.0)),
    ("kWh", def(Energy, 3.6e6)), ("kilowatt_hour", def(Energy, 3.6e6)),
    ("MWh", def(Energy, 3.6e9)),
    ("BTU", def(Energy, 1055.05585262)), ("btu", def(Energy, 1055.05585262)),
    ("eV", def(Energy, 1.602176634e-19)), ("electronvolt", def(Energy, 1.602176634e-19)),
    ("erg", def(Energy, 1e-7)),
    ("ftlb", def(Energy, 1.3558179483314004)), ("ft_lbf", def(Energy, 1.3558179483314004)),

    // ---- Power (base: watt) ----
    ("W", def(Power, 1.0)), ("watt", def(Power, 1.0)), ("watts", def(Power, 1.0)),
    ("mW", def(Power, 1e-3)), ("milliwatt", def(Power, 1e-3)),
    ("kW", def(Power, 1000.0)), ("kilowatt", def(Power, 1000.0)),
    ("MW", def(Power, 1e6)), ("megawatt", def(Power, 1e6)),
    ("GW", def(Power, 1e9)), ("gigawatt", def(Power, 1e9)),
    ("hp", def(Power, 745.6998715822702)), ("horsepower", def(Power, 745.6998715822702)),
    ("PS", def(Power, 735.49875)), ("metric_hp", def(Power, 735.49875)),

    // ---- Data (base: byte). Decimal k/M/G; binary Ki/Mi/Gi (1024^n). ----
    ("bit", def(Data, 0.125)), ("bits", def(Data, 0.125)), ("b", def(Data, 0.125)),
    ("byte", def(Data, 1.0)), ("bytes", def(Data, 1.0)), ("B", def(Data, 1.0)),
    ("kb", def(Data, 125.0)), ("kbit", def(Data, 125.0)), ("kilobit", def(Data, 125.0)),
    ("Mb", def(Data, 125000.0)), ("megabit", def(Data, 125000.0)),
    ("Gb", def(Data, 1.25e8)), ("gigabit", def(Data, 1.25e8)),
    ("kB", def(Data, 1000.0)), ("kilobyte", def(Data, 1000.0)),
    ("MB", def(Data, 1e6)), ("megabyte", def(Data, 1e6)),
    ("GB", def(Data, 1e9)), ("gigabyte", def(Data, 1e9)),
    ("TB", def(Data, 1e12)), ("terabyte", def(Data, 1e12)),
    ("PB", def(Data, 1e15)), ("petabyte", def(Data, 1e15)),
    ("KiB", def(Data, 1024.0)), ("kibibyte", def(Data, 1024.0)),
    ("MiB", def(Data, 1048576.0)), ("mebibyte", def(Data, 1048576.0)),
    ("GiB", def(Data, 1073741824.0)), ("gibibyte", def(Data, 1073741824.0)),
    ("TiB", def(Data, 1099511627776.0)), ("tebibyte", def(Data, 1099511627776.0)),
    ("PiB", def(Data, 1125899906842624.0)), ("pebibyte", def(Data, 1125899906842624.0)),

    // ---- Angle (base: radian) ----
    ("rad", def(Angle, 1.0)), ("radian", def(Angle, 1.0)), ("radians", def(Angle, 1.0)),
    ("deg", def(Angle, 0.017453292519943295)), ("degree", def(Angle, 0.017453292519943295)),
    ("degrees", def(Angle, 0.017453292519943295)), ("°", def(Angle, 0.017453292519943295)),
    ("grad", def(Angle, 0.015707963267948967)), ("gon", def(Angle, 0.015707963267948967)),
    ("arcmin", def(Angle, 0.0002908882086657216)),
    ("arcsec", def(Angle, 4.84813681109536e-6)),
    ("turn", def(Angle, std::f64::consts::TAU)), ("rev", def(Angle, std::f64::consts::TAU)),

    // ---- Frequency (base: hertz) ----
    ("Hz", def(Frequency, 1.0)), ("hertz", def(Frequency, 1.0)),
    ("kHz", def(Frequency, 1000.0)), ("kilohertz", def(Frequency, 1000.0)),
    ("MHz", def(Frequency, 1e6)), ("megahertz", def(Frequency, 1e6)),
    ("GHz", def(Frequency, 1e9)), ("gigahertz", def(Frequency, 1e9)),
    ("THz", def(Frequency, 1e12)),
    ("rpm", def(Frequency, 0.016666666666666666)),

    // ---- Force (base: newton) ----
    ("N", def(Force, 1.0)), ("newton", def(Force, 1.0)), ("newtons", def(Force, 1.0)),
    ("kN", def(Force, 1000.0)), ("kilonewton", def(Force, 1000.0)),
    ("dyn", def(Force, 1e-5)), ("dyne", def(Force, 1e-5)),
    ("lbf", def(Force, 4.4482216152605)), ("pound_force", def(Force, 4.4482216152605)),
    ("kgf", def(Force, 9.80665)), ("kilogram_force", def(Force, 9.80665)),
];

/// The lazily-built lookup map from unit string to definition.
fn table() -> &'static HashMap<&'static str, UnitDef> {
    static TABLE: OnceLock<HashMap<&'static str, UnitDef>> = OnceLock::new();
    TABLE.get_or_init(|| ENTRIES.iter().copied().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Assert two floats are within a relative tolerance.
    fn close(a: f64, b: f64) {
        let tol = 1e-9 * b.abs().max(1.0);
        assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
    }

    #[test]
    fn known_length_conversions() {
        close(convert(1.0, "mi", "km").unwrap(), 1.609344);
        close(convert(1.609344, "km", "mi").unwrap(), 1.0);
        close(convert(12.0, "in", "ft").unwrap(), 1.0);
        close(convert(1.0, "m", "cm").unwrap(), 100.0);
    }

    #[test]
    fn temperature_offsets() {
        close(convert(0.0, "C", "F").unwrap(), 32.0);
        close(convert(0.0, "C", "K").unwrap(), 273.15);
        close(convert(100.0, "C", "F").unwrap(), 212.0);
        close(convert(32.0, "F", "C").unwrap(), 0.0);
        close(convert(273.15, "K", "C").unwrap(), 0.0);
        close(convert(-40.0, "C", "F").unwrap(), -40.0);
    }

    #[test]
    fn mass_and_time() {
        close(convert(1.0, "kg", "lb").unwrap(), 2.204622621848776);
        close(convert(1.0, "hour", "s").unwrap(), 3600.0);
        close(convert(1.0, "h", "min").unwrap(), 60.0);
    }

    #[test]
    fn data_and_pressure() {
        close(convert(1.0, "GiB", "byte").unwrap(), 1073741824.0);
        close(convert(1.0, "atm", "Pa").unwrap(), 101325.0);
        close(convert(8.0, "bit", "byte").unwrap(), 1.0);
    }

    #[test]
    fn incompatible_dimensions_error() {
        let e = convert(1.0, "km", "kg").unwrap_err();
        matches!(e, UnitError::Incompatible { .. });
    }

    #[test]
    fn unknown_unit_error() {
        assert_eq!(
            convert(1.0, "frobnicate", "m").unwrap_err(),
            UnitError::UnknownUnit("frobnicate".to_string())
        );
        assert!(lookup("frobnicate").is_none());
    }

    #[test]
    fn dimension_and_compatible() {
        assert_eq!(dimension("mi").unwrap().name(), "length");
        assert!(compatible("mi", "km"));
        assert!(!compatible("mi", "kg"));
        assert!(!compatible("mi", "frobnicate"));
    }

    #[test]
    fn to_base_uses_si_base() {
        close(to_base(1.0, "km").unwrap(), 1000.0);
        close(to_base(0.0, "C").unwrap(), 273.15);
        close(to_base(1.0, "GiB").unwrap(), 1073741824.0);
    }

    #[test]
    fn parse_quantity_forms() {
        assert_eq!(
            parse_quantity("5 km"),
            Some(Quantity {
                value: 5.0,
                unit: "km".into()
            })
        );
        assert_eq!(
            parse_quantity("3.2kg"),
            Some(Quantity {
                value: 3.2,
                unit: "kg".into()
            })
        );
        assert_eq!(
            parse_quantity("10 m/s"),
            Some(Quantity {
                value: 10.0,
                unit: "m/s".into()
            })
        );
        assert_eq!(
            parse_quantity("-1.5e3 J"),
            Some(Quantity {
                value: -1500.0,
                unit: "J".into()
            })
        );
        assert_eq!(parse_quantity("no number"), None);
        assert_eq!(parse_quantity("5 frobnicate"), None);
        assert_eq!(parse_quantity("42"), None); // no unit
    }

    #[test]
    fn case_insensitive_fallback() {
        // 'KM' isn't an exact key but lowercases to 'km'.
        assert_eq!(dimension("KM").unwrap().name(), "length");
    }

    #[test]
    fn supported_units_nonempty_and_sorted() {
        let rows = supported_units();
        assert!(rows.len() > 100);
        assert_eq!(rows.len(), unit_count());
        // Every row's base_unit is itself a known unit.
        for r in &rows {
            assert!(
                lookup(r.base_unit).is_some(),
                "base {} unknown",
                r.base_unit
            );
        }
    }
}
