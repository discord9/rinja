use std::collections::HashMap;

use crate::Rule;
use pest::{Span, iterators::{Pair, Pairs}, prec_climber::{PrecClimber, Assoc, Operator}};
use serde_json::{value, Value};
use lazy_static::lazy_static;

#[derive(Debug)]
#[allow(dead_code)]
enum RendererErrorVariant{
    VarNotDefined
}
#[derive(Debug)]
#[allow(dead_code)]
pub struct RendererError{
    error: RendererErrorVariant,
    location: pest::error::InputLocation,
    line_col: pest::error::LineColLocation
}

lazy_static! {
    static ref PREC_CLIMBER: PrecClimber<Rule> = {
        use Rule::*;
        use Assoc::*;

        PrecClimber::new(vec![
            Operator::new(cmp, Left),
            Operator::new(add, Left) | Operator::new(subtract, Left),
            Operator::new(multiply, Left) | Operator::new(divide, Left),
            Operator::new(power, Right)
        ])
    };
}

pub trait Visitor<T,E> {
    //fn visit_block(&mut self, s:&Pairs<Rule>)->T;
    fn visit_expr(&mut self, s:Pairs<Rule>)->Result<T,E>;
}

type BuiltIn = Box<dyn Fn(value::Value)->value::Value>;
struct Interpreter{
    env: value::Value,// not ref because it may be modify by {% set lvalue = expr %}
    built_in_fn: HashMap<String, BuiltIn>
}

impl Interpreter{
    fn new(env: value::Value)->Self{
        use value::Value;
        let built_in_fn = HashMap::from([
            ("existIn".to_string(), Box::new(|a|a) as BuiltIn)
        ]);
        Self{
            env,
            built_in_fn
        }
    }
}
impl Visitor<value::Value, RendererError> for Interpreter{
    fn visit_expr(&mut self, expr:Pairs<Rule>)->Result<value::Value, RendererError> {
        use value::Value;
        let primary = |pair: Pair<Rule>|{
            // can direct unwrap because pset ensure it wouldn't have syntax error
            let res = match pair.as_rule(){
                Rule::num => serde_json::from_str(pair.as_str()).unwrap(),
                Rule::expr => self.visit_expr(pair.into_inner()).unwrap(),
                _ => unimplemented!()
            };
            res
        };
        let infix = |lhs: Value, op: Pair<Rule>, rhs: Value|{
            let (lhs, rhs) = {
                match (lhs.is_number(),rhs.is_number()){
                    (true, true) => (lhs.as_f64().unwrap(), rhs.as_f64().unwrap()),
                    _ => unimplemented!()
                }
            };
            match op.as_rule(){
                Rule::add      => Value::from(lhs + rhs),
                Rule::subtract => Value::from(lhs - rhs),
                Rule::multiply => Value::from(lhs * rhs),
                Rule::divide   => Value::from(lhs / rhs),
                Rule::power    => Value::from(lhs.powf(rhs)),
                Rule::cmp      => {
                    let res = 
                    match op.into_inner().next().unwrap().as_rule(){
                        Rule::lt => lhs < rhs,
                        Rule::gt => lhs > rhs,
                        Rule::ne => lhs != rhs,
                        Rule::eq => lhs == rhs,
                        Rule::ngt => lhs <= rhs,
                        Rule::nlt => lhs >= rhs,
                        _ => unreachable!()
                    };
                    Value::from(res)
                }
                _ => unimplemented!()
            }
        };
        Ok(PREC_CLIMBER.climb(expr, primary, infix))
    }
}

#[test]
fn test_num_expr(){
    use crate::{RinjaParser, Parser};
    let res = RinjaParser::parse(Rule::expr, "1+2*3^2!=18");
    println!("{:?}", res);
    let mut interp = Interpreter::new();
    println!("{:?}", interp.visit_expr(res.unwrap()));
}