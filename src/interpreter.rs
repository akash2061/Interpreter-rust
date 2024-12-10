use std::{cell::RefCell, rc::Rc};

use crate::{native, Environment, Expression, LoxFunction, Statement, Token, TokenType, Value};

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct InterpreterError {
    pub token: Option<Token>,
    pub message: String,
}

pub type ExecuteInterpreterResult = Result<Option<Value>, InterpreterError>;
pub type EvaluateInterpreterResult = Result<Value, InterpreterError>;

#[derive(Debug)]
pub struct Interpreter {
    pub globals: Environment,
    pub environment: Environment,
}

impl Interpreter {
    pub fn new() -> Self {
        let mut environment = Environment::new();

        environment.define(
            "clock".into(),
            Value::Function(Rc::new(RefCell::new(native::ClockFunction {}))),
        );

        Interpreter {
            globals: environment.clone(),
            environment,
        }
    }

    pub fn interpret(&mut self, statements: Vec<Statement>) -> ExecuteInterpreterResult {
        for statement in statements {
            self.execute(statement)?;
        }

        Ok(None)
    }

    pub fn execute(&mut self, statement: Statement) -> ExecuteInterpreterResult {
        match statement {
            Statement::Expression(expression) => {
                self.evaluate(expression)?;

                Ok(None)
            }
            Statement::Function {
                name,
                parameters,
                body,
            } => {
                let function = LoxFunction {
                    name,
                    parameters,
                    body,
                    closure: self.environment.clone(),
                };

                self.environment.define(
                    function.get_name().into(),
                    Value::Function(Rc::new(RefCell::new(function))),
                );

                Ok(None)
            }
            Statement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let result = self.evaluate(condition)?;

                if self.is_truthy(result) {
                    Ok(self.execute(*then_branch)?)
                } else if let Some(statement) = else_branch {
                    Ok(self.execute(*statement)?)
                } else {
                    Ok(None)
                }
            }
            Statement::Print(expression) => {
                match self.evaluate(expression)? {
                    Value::Number(value) => println!("{value}"),
                    value => println!("{value}"),
                }

                Ok(None)
            }
            Statement::Variable { name, initializer } => {
                let mut value = Value::Nil;
                if let Some(expression) = initializer {
                    value = self.evaluate(expression)?;
                }

                self.environment.define(name.lexeme, value);

                Ok(None)
            }
            Statement::Return { keyword: _, value } => {
                if let Some(expression) = value {
                    return Ok(Some(self.evaluate(expression)?));
                }

                Ok(Some(Value::Nil))
            }
            Statement::While { condition, body } => {
                loop {
                    let is_true = self.evaluate(condition.clone())?;

                    if !self.is_truthy(is_true) {
                        break;
                    }

                    if let Some(returned) = self.execute(*body.clone())? {
                        return Ok(Some(returned));
                    }
                }

                Ok(None)
            }
            Statement::Block(statements) => {
                Ok(self.execute_block(statements, self.environment.enclose())?)
            }
        }
    }

    pub fn execute_block(
        &mut self,
        statements: Vec<Statement>,
        environment: Environment,
    ) -> ExecuteInterpreterResult {
        let previous = self.environment.clone();
        self.environment = environment;

        for statement in statements {
            match self.execute(statement) {
                Err(error) => {
                    self.environment = previous;
                    return Err(error);
                }
                Ok(Some(value)) => {
                    self.environment = previous;
                    return Ok(Some(value));
                }
                Ok(None) => {}
            }
        }

        self.environment = previous;
        Ok(None)
    }

    pub fn evaluate(&mut self, expression: Expression) -> EvaluateInterpreterResult {
        match expression {
            Expression::Literal(literal) => Ok(literal.into()),
            Expression::Grouping(child) => self.evaluate(*child),
            Expression::Unary { operator, right } => {
                let right_child = self.evaluate(*right)?;

                match operator.token_type {
                    TokenType::Bang => Ok(Value::Boolean(!self.is_truthy(right_child))),
                    TokenType::Minus => Ok(Value::Number(
                        -self.check_number_operand(&operator, &right_child)?,
                    )),
                    _ => panic!("unreachable"),
                }
            }
            Expression::Binary {
                left,
                operator,
                right,
            } => {
                let left_child = self.evaluate(*left)?;
                let right_child = self.evaluate(*right)?;

                match operator.token_type {
                    TokenType::Slash => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Number(x / y));
                    }
                    TokenType::Star => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Number(x * y));
                    }
                    TokenType::Minus => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Number(x - y));
                    }
                    TokenType::Plus => {
                        if let (Value::Number(a), Value::Number(b)) = (&left_child, &right_child) {
                            return Ok(Value::Number(*a + *b));
                        }

                        if let (Value::String(a), Value::String(b)) = (&left_child, &right_child) {
                            let mut output: String = a.as_str().into();
                            output.push_str(b);

                            return Ok(Value::String(Rc::new(output)));
                        }

                        Err(InterpreterError {
                            token: Some(operator.clone()),
                            message: "Operands must be two numbers or two strings.".into(),
                        })
                    }
                    TokenType::Greater => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Boolean(x > y));
                    }
                    TokenType::GreaterEqual => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Boolean(x >= y));
                    }
                    TokenType::Less => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Boolean(x < y));
                    }
                    TokenType::LessEqual => {
                        let (x, y) =
                            self.check_number_operands(&operator, &left_child, &right_child)?;

                        return Ok(Value::Boolean(x <= y));
                    }
                    TokenType::BangEqual => Ok(Value::Boolean(left_child != right_child)),
                    TokenType::EqualEqual => Ok(Value::Boolean(left_child == right_child)),
                    _ => panic!("unreachable"),
                }
            }
            Expression::Variable(name) => {
                return self.environment.get(&name);
            }
            Expression::Assign { name, right } => {
                let value = self.evaluate(*right)?;

                self.environment.assign(&name, &value)?;

                return Ok(value);
            }
            Expression::Logical {
                left,
                operator,
                right,
            } => {
                let left_value = self.evaluate(*left)?;
                let is_left_truthy = self.is_truthy(left_value.clone());

                match operator.token_type {
                    TokenType::Or => {
                        if is_left_truthy {
                            return Ok(left_value);
                        }

                        self.evaluate(*right)
                    }
                    TokenType::And => {
                        if !is_left_truthy {
                            return Ok(left_value);
                        }

                        self.evaluate(*right)
                    }
                    _ => panic!("unreachable"),
                }
            }
            Expression::Call {
                callee,
                parenthesis,
                arguments,
            } => {
                let callee_value = self.evaluate(*callee)?;

                let mut arguments_values: Vec<Value> = Vec::new();
                for argument in arguments {
                    let argument_value = self.evaluate(argument)?;
                    arguments_values.push(argument_value);
                }

                if let Value::Function(callable) = callee_value {
                    let arity = callable.borrow().arity();
                    if arguments_values.len() != arity {
                        return Err(InterpreterError {
                            token: Some(parenthesis.clone()),
                            message: format!(
                                "Expected {arity} arguments but got {}.",
                                arguments_values.len()
                            ),
                        });
                    }

                    let returned_value =
                        callable
                            .borrow()
                            .call(self, arguments_values, parenthesis)?;
                    return Ok(returned_value.unwrap_or(Value::Nil));
                } else {
                    Err(InterpreterError {
                        token: Some(parenthesis.clone()),
                        message: "Can only call functions and classes.".into(),
                    })
                }
            }
        }
    }

    pub fn is_truthy(&self, value: Value) -> bool {
        match value {
            Value::Nil => false,
            Value::Boolean(value) => value,
            _ => true,
        }
    }

    pub fn check_number_operand(
        &self,
        operator: &Token,
        operand: &Value,
    ) -> Result<f64, InterpreterError> {
        match operand {
            Value::Number(x) => Ok(*x),
            _ => Err(InterpreterError {
                token: Some(operator.clone()),
                message: "Operand must be a number.".into(),
            }),
        }
    }

    pub fn check_number_operands(
        &self,
        operator: &Token,
        left: &Value,
        right: &Value,
    ) -> Result<(f64, f64), InterpreterError> {
        match (left, right) {
            (Value::Number(x), Value::Number(y)) => Ok((*x, *y)),
            _ => Err(InterpreterError {
                token: Some(operator.clone()),
                message: "Operands must be a number.".into(),
            }),
        }
    }
}
