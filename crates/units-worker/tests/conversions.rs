//! Integration tests for the pure conversion engine against known constants.
//! These exercise `units-worker`'s public conversion API the same way the Arrow
//! adapters do, but without any Arrow/RPC plumbing.

use units_worker::units::{self, Quantity, UnitError};

fn close(a: f64, b: f64) {
    let tol = 1e-6 * b.abs().max(1.0);
    assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
}

#[test]
fn one_mile_is_1_609344_km() {
    close(units::convert(1.0, "mi", "km").unwrap(), 1.609344);
}

#[test]
fn celsius_fahrenheit_kelvin() {
    close(units::convert(0.0, "C", "F").unwrap(), 32.0);
    close(units::convert(0.0, "C", "K").unwrap(), 273.15);
    close(units::convert(32.0, "F", "K").unwrap(), 273.15);
}

#[test]
fn one_kg_is_2_2046226_lb() {
    close(units::convert(1.0, "kg", "lb").unwrap(), 2.2046226);
}

#[test]
fn one_hour_is_3600_s() {
    close(units::convert(1.0, "hour", "s").unwrap(), 3600.0);
}

#[test]
fn one_gib_is_1073741824_byte() {
    close(units::convert(1.0, "GiB", "byte").unwrap(), 1073741824.0);
}

#[test]
fn one_atm_is_101325_pa() {
    close(units::convert(1.0, "atm", "Pa").unwrap(), 101325.0);
}

#[test]
fn incompatible_is_an_error() {
    assert!(matches!(
        units::convert(1.0, "km", "kg"),
        Err(UnitError::Incompatible { .. })
    ));
}

#[test]
fn unknown_is_an_error_in_the_engine() {
    // The engine returns an error; the Arrow adapter maps unknown → NULL.
    assert!(matches!(
        units::convert(1.0, "nope", "m"),
        Err(UnitError::UnknownUnit(_))
    ));
}

#[test]
fn dimension_and_compatibility() {
    assert_eq!(units::dimension("mi").unwrap().name(), "length");
    assert!(units::compatible("mi", "km"));
    assert!(!units::compatible("mi", "kg"));
}

#[test]
fn parse_five_km() {
    assert_eq!(
        units::parse_quantity("5 km"),
        Some(Quantity {
            value: 5.0,
            unit: "km".into()
        })
    );
}
