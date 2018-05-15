#![feature(test)]

extern crate mvp;
extern crate test;

use mvp::parser::grammar::{self, CompleteStr};
use test::Bencher;

#[bench]
fn address(b: &mut Bencher) {
    b.iter(|| grammar::statement(CompleteStr("LDA $19")));
}

#[bench]
fn address_ambiguous_parse(b: &mut Bencher) {
    b.iter(|| grammar::statement(CompleteStr("LDA ($19)+2")));
}
