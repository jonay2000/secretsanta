use crate::builtins::get_builtins;
use crate::error::SantaError;
use crate::function::{ArgumentList, Function, ParameterList};
use crate::manual::{increment_manual_id, MANUAL_ID, CONDITIONALS, LOOPS, FUNCTIONS};
use crate::object::Object;
use crate::parser::Operator;
use crate::parser::{AstNode, BinaryOperator, UnaryOperator};
use colored::Colorize;
use std::collections::HashMap;
use std::rc::Rc;
use std::cell::RefCell;

#[derive(Debug)]
pub struct Scope<'s> {
    parent: Option<&'s mut Scope<'s>>,
    locals: HashMap<String, Object>,
}

impl<'s> Scope<'s> {
    pub fn new() -> Self {
        let mut res = Self::with_parent(None);
        get_builtins(&mut res);
        res
    }

    pub fn add_builtin_fn(
        &mut self,
        name: &str,
        parameters: ParameterList,
        function: fn(&mut Scope) -> Result<Object, SantaError>,
    ) {
        self.set_variable(
            name.into(),
            Object::Function(Function::Builtin(parameters, function)),
        );
    }

    pub fn with_parent(parent: Option<&'s mut Scope<'s>>) -> Self {
        Self {
            parent,
            locals: HashMap::new(),
        }
    }

    pub unsafe fn child(
        &mut self,
        closure: impl FnOnce(&mut Scope) -> Result<Object, SantaError>,
    ) -> Result<Object, SantaError> {
        // This unsafe function is to temporaroly create a second mutable reference
        // to self. This is something that should be just fine because in this time
        // I'm not using the original reference.

        let raw = self as *mut Scope;
        let mut s = Scope {
            locals: HashMap::new(),
            parent: Some(raw.as_mut().expect("Couldn't dereference")),
        };

        closure(&mut s)
    }

    fn find_variable(&mut self, name: &String) -> Option<&mut Object> {
        if self.locals.contains_key(name) {
            self.locals.get_mut(name)
        } else if let Some(ref mut parent) = self.parent {
            parent.find_variable(name)
        } else {
            None
        }
    }

    pub fn set_variable(&mut self, name: String, value: Object) {
        if let Some(var) = self.find_variable(&name) {
//            if unsafe {MANUAL_ID} == FUNCTIONS && name == "assert".into() {
//                value.call(
//                    ArgumentList::new(vec![Object::Boolean(false)])
//                );
//            }

            *var = value;
        } else {
            self.locals.insert(name, value);
        }
    }

    pub fn get_variable(&self, name: &String) -> Option<Object> {
        if let Some(i) = self.locals.get(name) {
            Some(i.to_owned())
        } else if let Some(parent) = &self.parent {
            parent.get_variable(name)
        } else {
            None
        }
    }

    pub fn load_arglist(&mut self, arglist: ArgumentList, paramlist: ParameterList) -> Result<(), SantaError> {
        let isvararg = if let Some(last) = paramlist.positional.last() {
          last.starts_with("*")
        } else {
            false
        };

        if !isvararg && arglist.positional.len() > paramlist.positional.len() {
            return Err(SantaError::InvalidOperationError {cause: "Too many arguments for function".into()});
        } else if !isvararg && arglist.positional.len() < paramlist.positional.len() {
            return Err(SantaError::InvalidOperationError {cause: "Not enough arguments for function".into()});
        }else if isvararg && paramlist.positional.len() > 1 && arglist.positional.len() < paramlist.positional.len() -1 {
            return Err(SantaError::InvalidOperationError {cause: "Not enough arguments for function".into()});
        }

        if !isvararg {
            for (p, arg) in paramlist.positional.iter().zip(arglist.positional.iter()) {
                if p.starts_with("*") {
                    return Err(SantaError::InvalidOperationError {cause: "Vararg definition not at the end of function parameterlist".into()});
                }
                self.set_variable(p.clone(), arg.clone());
            }
        } else {

            let (direct, var) = arglist.positional.split_at(paramlist.positional.len()-1);
            let (params, varargname) = paramlist.positional.split_at(paramlist.positional.len()-1);

            for (p, arg) in params.iter().zip(direct.iter()) {
                if p.starts_with("*") {
                    return Err(SantaError::InvalidOperationError {cause: "Multiple variable argument declarations in function signature".into()});
                }
                self.set_variable(p.clone(), arg.clone());
            }

            let mut vararg = vec![];
            for i in var {
                vararg.push(i.clone());
            }

            self.set_variable(varargname[0][1..].into(), Object::List(Rc::new(RefCell::new(vararg))));

        };

        Ok(())
    }
}

pub fn eval_node(node: &AstNode, scope: &mut Scope) -> Result<Object, SantaError> {
    match node {
        AstNode::None => Ok(Object::None),
        AstNode::Expression(operatortype) => match operatortype {
            Operator::Binary { operator, rhs, lhs } => {
                let rhs_eval = eval_node(rhs, scope)?;
                let lhs_eval = eval_node(lhs, scope)?;
                match operator {
                    BinaryOperator::Add => lhs_eval.add(&rhs_eval),
                    BinaryOperator::Multiply => lhs_eval.multiply(&rhs_eval),
                    BinaryOperator::Divide => lhs_eval.divide(&rhs_eval),
                    BinaryOperator::Subtract => lhs_eval.subtract(&rhs_eval),

                    BinaryOperator::Less => lhs_eval.less(&rhs_eval),
                    BinaryOperator::Greater => lhs_eval.greater(&rhs_eval),
                    BinaryOperator::LessEquals => lhs_eval.lessequals(&rhs_eval),
                    BinaryOperator::GreaterEquals => lhs_eval.greaterequals(&rhs_eval),
                    BinaryOperator::Equals => lhs_eval.equals(&rhs_eval),
                    BinaryOperator::NotEquals => lhs_eval.notequals(&rhs_eval),

                    BinaryOperator::Index => lhs_eval.index(&rhs_eval),
                }
            }
            Operator::Unary { operator, expr } => {
                let expr_eval = eval_node(expr, scope)?;

                match operator {
                    UnaryOperator::Negate => expr_eval.negate(),
                }
            }
        },
        AstNode::Assignment { name, expression, indexes } => {
            let evaluated = eval_node(expression, scope)?;

            match name.as_ref() {
                AstNode::Name(name) => {
                    if indexes.len() > 0 {
                        let mut curr = scope.get_variable(name).ok_or(SantaError::NoDefinitionError)?;
                        for i in indexes.iter().take(indexes.len() - 1) {
                            let value = eval_node(i.as_ref(), scope)?;
                            curr = curr.index(&value)?;
                        }
                        // We already checked that there was 1 item in the indexes list
                        curr.setindex(&eval_node(indexes.iter().last().unwrap().as_ref(), scope)?, &evaluated.clone())?;
                    } else {
                        scope.set_variable(name.clone(), evaluated.clone());
                    }

                    Ok(evaluated)
                }
                _ => Err(SantaError::InvalidOperationError {
                    cause: "Tried to assign to something that's not a variable name".into(),
                }),
            }
        }

        AstNode::List(list) => Ok(Object::List( Rc::new(RefCell::new(list.iter()
            .map(|i| eval_node(i, scope))
            .collect::<Result<Vec<Object>, SantaError>>()?)))),

        AstNode::Map(map) => Ok(Object::Map( Rc::new(RefCell::new(map.iter()
            .map(|i| Ok((eval_node(i.0.as_ref(), scope)?, eval_node(i.1.as_ref(), scope)?)))
            .collect::<Result<HashMap<Object, Object>, SantaError>>()?)))),

        AstNode::Integer(integer) => Ok(Object::Integer(integer.clone())),
        AstNode::Boolean(boolean) => Ok(Object::Boolean(boolean.clone())),
        AstNode::Float(float) => Ok(Object::Float(float.clone())),
        AstNode::String(string) => Ok(Object::String(string.clone())),
        AstNode::Name(string) => {
            Ok(scope
                .get_variable(string)
                .ok_or(SantaError::NoDefinitionError)?)
        }
        AstNode::Functioncall { value, args } => {
            let variable = eval_node(value, scope)?;
            let mut arguments = ArgumentList::new(vec![]);
            for i in args {
                arguments.positional.push(eval_node(i, scope)?)
            }

            variable.call(arguments)
        }
        AstNode::Function {
            name,
            parameterlist,
            code,
        } => {
            let func = Object::Function(Function::User(parameterlist.clone(), code.clone()));


            // If you gave the function a name, assign it to a variable with that name.
            if let AstNode::Name(name) = *name.clone() {
                scope.set_variable(name, func.clone());
            }

            Ok(func)
        }
        AstNode::WhileLoop { condition, code } => {
            if unsafe { MANUAL_ID } == LOOPS {
                println!("{}", "You used a while loop for the first time!".yellow());
                increment_manual_id();
            }

            let mut value = eval_node(condition.as_ref(), scope)?;

            if let Object::Boolean(_) = value {
                while let Object::Boolean(true) = value {
                    unsafe {
                        scope.child(|subscope| eval_block_with_scope(&code, subscope))?;
                    }
                    value = eval_node(condition.as_ref(), scope)?;
                }
                Ok(Object::None)
            } else {
                Err(SantaError::InvalidOperationError {
                    cause: "Expresion in if statement not a boolean.".into(),
                })
            }
        }
        AstNode::IfStatement {
            condition,
            code,
            elsecode,
        } => {
            if unsafe { MANUAL_ID } == CONDITIONALS {
                println!(
                    "{}",
                    "You used an if statement for the first time!".yellow()
                );
                increment_manual_id();
            }

            if let Object::Boolean(value) = eval_node(condition, scope)? {
                unsafe {
                    scope.child(|subscope| {
                        if value {
                            eval_block_with_scope(code.as_ref(), subscope)
                        } else if let Some(elsecode) = elsecode{
                            eval_block_with_scope(elsecode.as_ref(), subscope)
                        } else {
                            Ok(Object::None)
                        }
                    })
                }
            } else {
                Err(SantaError::InvalidOperationError {
                    cause: "Expresion in if statement not a boolean.".into(),
                })
            }
        }
        AstNode::Return(expr) => Err(SantaError::ReturnException {
            value: eval_node(expr, scope)?,
        }),
    }
}

pub fn eval(ast: Vec<Box<AstNode>>) {
    let mut scope = Scope::new();

    eval_with_scope(ast, &mut scope);
}

pub fn eval_block_with_scope(
    ast: &Vec<Box<AstNode>>,
    scope: &mut Scope,
) -> Result<Object, SantaError> {
    let mut last_answer = Object::None;
    for node in ast {
        match eval_node(node.as_ref(), scope) {
            Err(SantaError::ReturnException { value }) => {
                return Ok(value);
            }
            Err(e) => {
                return Err(e);
            }
            Ok(i) => {
                last_answer = i;
            }
        };
    }

    Ok(last_answer)
}

pub fn eval_with_scope(ast: Vec<Box<AstNode>>, scope: &mut Scope) -> Object {
    let mut last_answer = Object::None;
    for node in ast {
        match eval_node(node.as_ref(), scope) {
            Err(SantaError::ReturnException { value }) => {
                return value;
            }
            Err(e) => {
                Err(e).unwrap()
            }
            Ok(i) => {
                last_answer = i;
            }
        };
    }

    last_answer
}



pub fn eval_with_scope_err(ast: Vec<Box<AstNode>>, scope: &mut Scope) -> Result<Object, SantaError> {
    let mut last_answer = Object::None;
    for node in ast {
        match eval_node(node.as_ref(), scope) {
            Err(SantaError::ReturnException { value }) => {
                return Ok(value);
            }
            Err(e) => {
                return Err(e);
            }
            Ok(i) => {
                last_answer = i;
            }
        };
    }
    Ok(last_answer)
}