#![cfg(test)]
extern crate pest;
use crate::pest::{Parser};
use crate::{RinjaParser, Rule};

#[test]
fn manual_test(){
    // simple string
    let res = RinjaParser::parse(Rule::tmpl_unit, "123");
    println!("{:?}", res);
    assert!(res.is_ok());

    // simple expr
    let res = RinjaParser::parse(Rule::tmpl_unit, "123{{a}}bcd");
    assert!(res.is_ok());
    if res.is_err(){
        println!("{:?}", res.unwrap().as_str());
    }
    

    // simple expr that fail
    let res = RinjaParser::parse(Rule::tmpl_unit, "123{{1a}}bcd");
    assert!(res.is_err());
    if res.is_ok(){
        println!("{:?}", res.expect_err("Should fail as illegal ident"));
    }

    let res = RinjaParser::parse(Rule::tmpl_unit, "{% for a in b %}{% endfor %}");
    println!("{:?}", res);
}