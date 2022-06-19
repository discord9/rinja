use std::collections::HashMap;

use crate::Rule;
use lazy_static::lazy_static;
use pest::{
    iterators::{Pair, Pairs},
    prec_climber::{Assoc, Operator, PrecClimber},
    Span,
};
use serde_json::value;

#[derive(Debug)]
#[allow(dead_code)]
enum RendererErrorVariant {
    VarNotDefined,
}
#[derive(Debug)]
#[allow(dead_code)]
pub struct RendererError<'a> {
    error_msg: String,
    span: Span<'a>,
}

impl<'a> RendererError<'a> {
    fn new_from_span(error_msg: String, span: Span<'a>) -> Self {
        Self { error_msg, span }
    }
}

lazy_static! {
    static ref PREC_CLIMBER: PrecClimber<Rule> = {
        use Assoc::*;
        use Rule::*;

        PrecClimber::new(vec![
            Operator::new(cmp, Left),
            Operator::new(add, Left) | Operator::new(subtract, Left),
            Operator::new(multiply, Left) | Operator::new(divide, Left),
            Operator::new(power, Right),
        ])
    };
}

pub trait Visitor<T, E> {
    //fn visit_block(&mut self, s:&Pairs<Rule>)->T;
    fn visit_expr(&mut self, s: Pairs<Rule>) -> Result<T, E>;
    fn visit_tmpl_unit(&mut self, s: Pairs<Rule>);
}

type BuiltIn = Box<dyn Fn(Vec<value::Value>) -> value::Value>;
struct Interpreter {
    env: value::Value, // not ref because it may be modify by {% set lvalue = expr %}
    built_in_fn: HashMap<String, BuiltIn>,
    render_result: String,
}

impl Interpreter {
    fn new(env: value::Value) -> Self {
        use value::Value;
        let built_in_fn = HashMap::from([(
            "existsIn".to_string(),
            Box::new(|args: Vec<Value>| -> Value { args.get(0).unwrap().to_owned() }) as BuiltIn,
        )]);
        Self {
            env,
            built_in_fn,
            render_result: String::new(),
        }
    }
}
impl Visitor<value::Value, RendererError<'static>> for Interpreter {
    fn visit_expr(&mut self, expr: Pairs<Rule>) -> Result<value::Value, RendererError<'static>> {
        use value::Value;
        let primary = |pair: Pair<Rule>| {
            // can direct unwrap because pset ensure it wouldn't have syntax error
            let res = match pair.as_rule() {
                Rule::num => serde_json::from_str(pair.as_str()).unwrap(),
                Rule::ident => match self.env.get(pair.as_str()) {
                    Some(v) => v.to_owned(),
                    None => {
                        let (l, c) = pair.as_span().start_pos().line_col();
                        // Should panic because its interpreting so recover from error is not a good idea?
                        panic!(
                            "Variable {} not found!(At {}:{})",
                            pair.as_str().to_string(),
                            l,
                            c
                        );
                    }
                },
                Rule::expr => self.visit_expr(pair.into_inner()).unwrap(),
                _ => unimplemented!(),
            };
            res
        };
        let infix = |lhs: Value, op: Pair<Rule>, rhs: Value| {
            let (lhs, rhs) = {
                //let (lhs, rhs) = (lhs.unwrap(), rhs.unwrap());
                match (lhs.is_number(), rhs.is_number()) {
                    (true, true) => (lhs.as_f64().unwrap(), rhs.as_f64().unwrap()),
                    _ => unimplemented!(),
                }
            };
            match op.as_rule() {
                Rule::add => Value::from(lhs + rhs),
                Rule::subtract => Value::from(lhs - rhs),
                Rule::multiply => Value::from(lhs * rhs),
                Rule::divide => Value::from(lhs / rhs),
                Rule::power => Value::from(lhs.powf(rhs)),
                Rule::cmp => {
                    let res = match op.into_inner().next().unwrap().as_rule() {
                        Rule::lt => lhs < rhs,
                        Rule::gt => lhs > rhs,
                        Rule::ne => lhs != rhs,
                        Rule::eq => lhs == rhs,
                        Rule::ngt => lhs <= rhs,
                        Rule::nlt => lhs >= rhs,
                        _ => unreachable!(),
                    };
                    Value::from(res)
                }
                _ => unimplemented!(),
            }
        };
        Ok(PREC_CLIMBER.climb(expr, primary, infix))
    }
    fn visit_tmpl_unit(&mut self, unit: Pairs<Rule>) {
        //println!("Unit:{:?}", unit);
        let unit = unit.to_owned().next().unwrap();
        println!("Unit:{:?}", unit);
        self.render_result = String::with_capacity(unit.as_str().len() * 2);
        for tmpl_section in unit.into_inner() {
            match tmpl_section.as_rule() {
                Rule::tmpl_literal => self.render_result.push_str(tmpl_section.as_str()),
                Rule::expr => {
                    let eval_res = self.visit_expr(tmpl_section.into_inner()).unwrap();
                    if eval_res.is_string() {
                        self.render_result.push_str(eval_res.as_str().unwrap());
                    } else if eval_res.is_number() {
                        let res = if eval_res.is_i64() {
                            eval_res.as_i64().unwrap().to_string()
                        } else if eval_res.is_f64() {
                            eval_res.as_f64().unwrap().to_string()
                        }else{unimplemented!()};
                        self.render_result.push_str(format!("{}", res).as_str())
                    } else {
                        unimplemented!()
                    }
                }
                Rule::EOI => continue,
                _ => {
                    println!("{:?} Not yet support!", tmpl_section.as_rule());
                    unimplemented!()
                },
            }
        }
    }
}

#[test]
fn test_num_expr() {
    use crate::{Parser, RinjaParser};
    let res = RinjaParser::parse(Rule::expr, "1+a*3^2");
    //println!("{:?}", res);
    let mut interp = Interpreter::new(serde_json::from_str(r#"{"a":42}"#).unwrap());
    println!("{:?}", interp.visit_expr(res.unwrap()));
    
    let res = RinjaParser::parse(Rule::tmpl_unit, "aa{{ a }}bb");
    println!("{:?}", res.to_owned().unwrap());
    let mut interp = Interpreter::new(serde_json::from_str(r#"{"a":42}"#).unwrap());
    interp.visit_tmpl_unit(res.unwrap());
    println!("Renderer Result: {}", interp.render_result);
}
