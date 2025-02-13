use rust_chain::{check, Asset};
pub(crate) fn ipow(base: i64, exp: usize) -> u128 {
    if exp == 0 {
        return 1;
    }

    let mut result = base;
    for _ in 0..(exp - 1) {
        result *= base;
    }

    result as u128
}

pub(crate) fn divide(a: Asset, b: Asset) -> Asset {
    check(b.amount() > 0, "can\'t divide by zero");
    check(a.symbol().precision() == b.symbol().precision(), "same precision only");

    let _a = a.amount() as u128;
    let _b = b.amount() as u128;

    // perform operation and add extra precision destroyed by it
    let _result = ((_a * ipow(10, b.symbol().precision())) / _b) as i64;

    Asset::new(_result, a.symbol())
}

pub(crate) fn multiply(a: Asset, b: Asset) -> Asset {
    check(a.symbol().precision() == b.symbol().precision(), "same precision only");
    let _a = a.amount() as i128;
    let _b = b.amount() as i128;

    // perform operation and remove extra precision created by it
    let _result = ((_a * _b) / ipow(10, a.symbol().precision()) as i128) as i64;

    Asset::new(_result, a.symbol())
}