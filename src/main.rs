extern crate pest;
#[macro_use]
extern crate pest_derive;
use pest::{Parser, iterators::Pairs};

mod test;
mod interpreter;
#[derive(Parser)]
#[grammar = "../pest/rinja.pest"]
pub struct RinjaParser;


fn main() {
    let res = RinjaParser::parse(Rule::tmpl_unit, "123");
    println!("{:?}",res);
    println!("Hello, world!");
}
