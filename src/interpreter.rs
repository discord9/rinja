use std::{borrow::BorrowMut, collections::HashMap};

use crate::Rule;
use lazy_static::lazy_static;
use pest::{
    iterators::{Pair, Pairs},
    prec_climber::{Assoc, Operator, PrecClimber},
    Span,
};
use serde_json::{json, value};

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

pub trait Visitor {
    //fn visit_block(&mut self, s:&Pairs<Rule>)->T;
    fn visit_expr(&mut self, s: Pairs<Rule>);
    fn visit_single_stmt(&mut self, s: Pairs<Rule>);
    fn visit_if_block(&mut self, s: Pairs<Rule>);
    fn visit_for_block(&mut self, s: Pairs<Rule>);
    fn visit_block(&mut self, s: Pairs<Rule>);
    fn visit_tmpl_unit(&mut self, s: Pairs<Rule>);
}

type BuiltIn = Box<dyn Fn(Vec<value::Value>) -> value::Value>;
struct Interpreter {
    env: value::Value, // not ref because it may be modify by {% set lvalue = expr %}
    tmp_var: Vec<value::Value>, // search in tmp_var first(like in for statement)
    built_in_fn: HashMap<String, BuiltIn>,
    render_result: String,
}

impl Interpreter {
    fn new(env: value::Value, cap: usize) -> Self {
        use value::Value;
        let built_in_fn = HashMap::from([(
            "existsIn".to_string(),
            Box::new(|args: Vec<Value>| -> Value { args.get(0).unwrap().to_owned() }) as BuiltIn,
        )]);
        Self {
            env,
            tmp_var: Vec::new(),
            built_in_fn,
            render_result: String::with_capacity(cap),
        }
    }

    /*
    fn panic_renderer_error(&self, pair: Pair<Rule>) -> ! {
        let (l, c) = pair.as_span().start_pos().line_col();
        // Should panic because its interpreting so recover from error is not a good idea?
        panic!(
            "Variable {} not found!(At {}:{})",
            pair.as_str().to_string(),
            l,
            c
        );
    }
    */

    /// panic when encounter runtime error return type is `!` means divergence and never return (for type infer)
    fn panic_renderer_error(pair: Pair<Rule>) -> ! {
        let (l, c) = pair.as_span().start_pos().line_col();
        // Should panic because its interpreting so recover from error is not a good idea?
        panic!(
            "Variable {} not found!(At {}:{})",
            pair.as_str().to_string(),
            l,
            c
        );
    }

    // give a ident return value
    fn get_val_from_env(&self, pair: Pair<Rule>) -> value::Value {
        for t in self.tmp_var.iter().rev() {
            if let Some(v) = t.get(pair.as_str()) {
                return v.to_owned();
            }
        }
        match self.env.get(pair.as_str()) {
            Some(v) => v.to_owned(),
            None => Interpreter::panic_renderer_error(pair),
        }
    }

    // get left value mut-ly
    fn get_lvalue_mut(&mut self, lval: Pair<Rule>) -> &mut value::Value {
        let first = lval.into_inner().next().unwrap();
        match first.as_rule() {
            Rule::ident => match self.env.get_mut(first.as_str()) {
                Some(v) => v,
                None => Interpreter::panic_renderer_error(first),
            },
            Rule::subs => self.get_subs_mut(first),
            _ => unreachable!(),
        }
    }
    fn get_subs_mut(&mut self, subs: Pair<Rule>) -> &mut value::Value {
        let mut query = &mut self.env;
        // iter over subs to find out actual value
        for key in subs.into_inner() {
            // subs can be ".a" or "["a"]"
            match key.as_rule() {
                Rule::ident => match query.get_mut(key.as_str()) {
                    Some(v) => query = v,
                    None => Interpreter::panic_renderer_error(key),
                },
                Rule::str => {
                    let res = key.clone().into_inner().next().unwrap().as_str();
                    {
                        match query.get_mut(res) {
                            Some(v) => query = v,
                            None => Interpreter::panic_renderer_error(key),
                        }
                    }
                }
                Rule::uint => {
                    let res = key.as_str().parse::<usize>().unwrap();
                    match query.get_mut(res) {
                        Some(v) => query = v,
                        None => Interpreter::panic_renderer_error(key),
                    }
                }
                _ => unreachable!(),
            }
        }
        query
    }
    fn get_subs(&self, subs: Pair<Rule>) -> &value::Value {
        let mut query = &self.env;
        // iter over subs to find out actual value
        for key in subs.into_inner() {
            // subs can be ".a" or "["a"]"
            match key.as_rule() {
                Rule::ident => match query.get(key.as_str()) {
                    Some(v) => query = v,
                    None => Interpreter::panic_renderer_error(key),
                },
                Rule::str => {
                    let res = self.eval_expr(Pairs::single(key.clone()));
                    if res.is_string() {
                        match query.get(res.as_str().unwrap()) {
                            Some(v) => query = v,
                            None => Interpreter::panic_renderer_error(key),
                        }
                    } else {
                        println!("{:?} Not yet support!", key.as_rule());
                        unimplemented!()
                    }
                }
                Rule::uint => {
                    let res = key.as_str().parse::<usize>().unwrap();
                    match query.get(res) {
                        Some(v) => query = v,
                        None => Interpreter::panic_renderer_error(key),
                    }
                }
                _ => unreachable!(),
            }
        }
        query
    }
    // evaluate a expression without changing environment
    fn eval_expr(&self, expr: Pairs<Rule>) -> value::Value {
        use value::Value;
        let primary = |pair: Pair<Rule>| match pair.as_rule() {
            Rule::num => serde_json::from_str(pair.as_str()).unwrap(),
            Rule::ident => self.get_val_from_env(pair),
            Rule::subs => self.get_subs(pair).to_owned(),
            // only ok for one child's case(which is true for prec_climber)
            Rule::expr => self.eval_expr(pair.into_inner()),
            _ => unimplemented!(),
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

        PREC_CLIMBER.climb(expr, primary, infix)
    }
}
impl Visitor for Interpreter {
    fn visit_expr(&mut self, expr: Pairs<Rule>) {
        //dbg!(expr.as_str());
        let eval_res = self.eval_expr(expr);
        //dbg!(eval_res.clone());
        if eval_res.is_string() {
            self.render_result.push_str(eval_res.as_str().unwrap());
        } else if eval_res.is_number() {
            let res = if eval_res.is_i64() {
                eval_res.as_i64().unwrap().to_string()
            } else if eval_res.is_f64() {
                eval_res.as_f64().unwrap().to_string()
            } else {
                unimplemented!()
            };
            //dbg!(res.clone());
            self.render_result.push_str(&res);
            //dbg!(self.render_result.clone());
        } else if eval_res.is_array() {
            println!(
                "Renderer array in template is not yet supported: {}",
                eval_res
            );
            unimplemented!()
        } else {
            println!("Expect str/num in expr:{:?}", eval_res);
            unimplemented!()
        }
    }
    fn visit_single_stmt(&mut self, stmt: Pairs<Rule>) {
        for stmt in stmt {
            match stmt.as_rule() {
                Rule::set_stmt => {
                    let mut it = stmt.into_inner();
                    let first = it.next().unwrap();
                    let second = it.next().unwrap();
                    let e = self.eval_expr(Pairs::single(second));
                    let lval = self.get_lvalue_mut(first);
                    // lval.clone_from(&e);
                    *lval = e.clone();
                }
                Rule::include_stmt => {}
                _ => unreachable!(),
            }
        }
    }
    fn visit_if_block(&mut self, block: Pairs<Rule>) {
        let mut if_flag: Option<bool> = None;
        let mut else_if_flag: Option<bool> = None;
        for body in block {
            match body.as_rule() {
                Rule::if_stmt => {
                    let pred = self.eval_expr(body.into_inner());
                    if pred.is_boolean() {
                        if_flag = Some(pred.as_bool().unwrap());
                    } else {
                        panic!("predicate only support bool!")
                    }
                }
                Rule::if_body => {
                    if let Some(true) = if_flag {
                        self.visit_tmpl_unit(body.into_inner());
                        break;
                    }
                }
                Rule::else_if_stmt => {
                    let pred = self.eval_expr(body.into_inner());
                    if pred.is_boolean() {
                        else_if_flag = Some(pred.as_bool().unwrap());
                    } else {
                        panic!("predicate only support bool!")
                    }
                }
                Rule::else_if_body => {
                    if let Some(true) = else_if_flag {
                        self.visit_tmpl_unit(body.into_inner());
                        break;
                    }
                }
                Rule::else_body => self.visit_tmpl_unit(body.into_inner()),
                _ => unreachable!(),
            }
        }
    }
    fn visit_for_block(&mut self, b: Pairs<Rule>) {
        let parent = match self.tmp_var.last().to_owned(){
            Some(v) => v.to_owned(),
            None => value::Value::Null
        };
        let current = json!({
            "loop":{
                "index": 0,
                "index1": 1,
                "is_first": true,
                "is_last": false,
                "parent": parent
            }
        });
        self.tmp_var.push(current);
        for b in b {
            match b.as_rule() {
                Rule::for_stmt => {
                    let mut for_stmt = b.into_inner();
                    let it = for_stmt.next().unwrap();
                    let expr = for_stmt.next().unwrap();
                    let expr = self.eval_expr(expr.into_inner());
                    if !expr.is_array() && !expr.is_object() {
                        panic!("Iterate over non-array/object is not support: {:?}", expr);
                    }
                    todo!()
                }
                Rule::for_body => {}
                _ => unimplemented!(),
            }
        }
        self.tmp_var.pop();
    }
    fn visit_block(&mut self, b: Pairs<Rule>) {
        for b in b {
            match b.as_rule() {
                Rule::if_block => self.visit_if_block(b.into_inner()),
                Rule::for_block => self.visit_for_block(b.into_inner()),
                _ => unreachable!(),
            }
        }
    }
    fn visit_tmpl_unit(&mut self, unit: Pairs<Rule>) {
        //println!("Unit:{:?}", unit);
        //let unit = unit.to_owned().next().unwrap();
        for tmpl_section in unit {
            //dbg!((tmpl_section.as_rule(), tmpl_section.as_str()));
            match tmpl_section.as_rule() {
                Rule::block => self.visit_block(tmpl_section.into_inner()),
                Rule::single_stmt => self.visit_single_stmt(tmpl_section.into_inner()),
                Rule::tmpl_literal => self.render_result.push_str(tmpl_section.as_str()),
                Rule::expr => self.visit_expr(tmpl_section.into_inner()),
                Rule::EOI => continue,
                _ => {
                    println!("{:?} Not yet support!", tmpl_section.as_rule());
                    unimplemented!()
                }
            }
        }
    }
}

#[test]
fn strange_bug() {
    use crate::{Parser, RinjaParser};
    let renderer_tmpl = r#"## set a = b
{{ a[1] }}
## if a[0] == 1
abc
## set a[0] = 42
## else if a[1] == 1
456
## else 
789
## endif
{{ a[0] }}
    "#;
    let res = RinjaParser::parse(Rule::tmpl_unit, renderer_tmpl);
    // println!("{:?}", res.to_owned().unwrap());
    let mut interp = Interpreter::new(
        serde_json::from_str(r#"{"a":43, "b":[0,1,2], "c":{"a":0}}"#).unwrap(),
        200,
    );
    interp.visit_tmpl_unit(res.unwrap());
    println!(
        "Renderer Template:\n{}\nRendererResult:\n{}",
        renderer_tmpl, interp.render_result
    );
}

#[test]
fn test_num_expr() {
    use crate::{Parser, RinjaParser};
    let res = RinjaParser::parse(Rule::expr, "1+a*3^2");
    //println!("{:?}", res);
    let interp = Interpreter::new(serde_json::from_str(r#"{"a":42}"#).unwrap(), 200);
    let res = interp.eval_expr(res.unwrap());
    //println!("{:?}", res);
    assert_eq!(res.as_f64().unwrap(), 379.0);

    let renderer_tmpl = "simple:{{ a }}, array:{{b[1]}},subs:{{c.a}}";
    let res = RinjaParser::parse(Rule::tmpl_unit, renderer_tmpl);
    //println!("{:?}", res.to_owned().unwrap());
    let mut interp = Interpreter::new(
        serde_json::from_str(r#"{"a":43, "b":[0,1,2], "c":{"a":0}}"#).unwrap(),
        200,
    );
    interp.visit_tmpl_unit(res.unwrap());
    println!(
        "Renderer Template:\n{}\nRendererResult:\n{}",
        renderer_tmpl, interp.render_result
    );
    assert_eq!(interp.render_result.as_str(), "simple:43, array:1,subs:0");

    let renderer_tmpl = r#"## set a = b
{{ a[1] }}
## if a[0] == 0
abc
## set a[0] = 42
## else if a[1] == 1
456
## else 
789
## endif
{{a[0] }}
    "#;
    let res = RinjaParser::parse(Rule::tmpl_unit, renderer_tmpl);
    //println!("{:?}", res.to_owned().unwrap());
    let mut interp = Interpreter::new(
        serde_json::from_str(r#"{"a":43, "b":[0,1,2], "c":{"a":0}}"#).unwrap(),
        200,
    );
    interp.visit_tmpl_unit(res.unwrap());
    println!(
        "Renderer Template:\n{}\nRendererResult:\n{}",
        renderer_tmpl, interp.render_result
    );
}
