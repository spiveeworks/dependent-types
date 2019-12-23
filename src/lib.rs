#[macro_use]
extern crate lalrpop_util;

pub mod ast;
mod indent_parser;

// why am I even using lalrpop for such a simple grammar
// f : (x1: A) -> (x2: B) -> (x3: C) -> D
// f x1 x2 x3 = a (b c (d e) f) g h
lalrpop_mod!(line_parser);

pub use indent_parser::ProgramParser;

struct Item {
    ty: Expr,
    def: Option<(usize, Expr)>,
}

pub struct Globals {
    names: Vec<String>,
    defs: Vec<Item>,
}

impl Globals {
    pub fn new() -> Globals {
        Globals { names: Vec::new(), defs: Vec::new() }
    }
}

pub fn type_check_all(globals: &mut Globals, programs: Vec<ast::Item>) {
    for item in &programs {
        let (name, item) = type_check_function(globals, item);
        println!("{}: {}", name, item.ty);
        globals.names.push(name);
        globals.defs.push(item);
    }

    print!("Successfully type-checked all items!\n\n");
}

fn type_check_function(
    globals: &Globals,
    fun: &ast::Item,
) -> (String, Item) {
    for _ in &fun.associated {
        unimplemented!();
    }
    if fun.annotation.is_none() {
        if fun.definition.is_none() {
            panic!("Found neither annotation nor definition?");
        } else if fun.definition.as_ref().unwrap().vars.len() > 0 {
            panic!("Terms with parameters must have a type annotation");
        } else {
            unimplemented!();
        }
    }
    let annotation = fun.annotation.as_ref().unwrap();
    let mut ty = convert_expr(
        &globals.names,
        &Default::default(),
        annotation.typ.clone()
    );
    sort_check_expr(&globals.defs, &Context::new(&[]), &ty);
    // maybe we want to store both eval and non-eval versions?
    eval(&globals.defs, &mut ty, 0);

    if fun.definition.is_none() {
        (annotation.name.clone(), Item { ty, def: None })
    } else {
        let definition = fun.definition.as_ref().unwrap();
        if annotation.name != definition.fname {
            panic!(
                "Annotation for {} was given alongside definition for {}",
                annotation.name,
                definition.fname,
            );
        }
        let var_names = &definition.vars;
        let param_num = var_names.len();

        let def = convert_expr(
            &globals.names,
            &Context::new(&var_names),
            definition.body.clone(),
        );

        if !annotation.is_post {
            let mut result = ty.clone();
            let bindings: Vec<_> = result
                .arrow_params
                .drain(0..param_num)
                .collect();

            type_check_expr(&globals.defs, &Context::new(&bindings), &def, &result);
        }

        (definition.fname.clone(), Item { ty, def: Some((param_num, def)) })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ident {
    Universe(usize),
    Global(usize),
    Local(usize),
}

#[derive(Clone, PartialEq)]
struct Expr {
    arrow_params: Vec<Expr>,
    head: Ident,
    tail: Vec<Expr>,
}

impl Expr {
    fn universe(l: usize) -> Self {
        Expr {
            arrow_params: Vec::new(),
            head: Ident::Universe(l),
            tail: Vec::new(),
        }
    }
    fn universe_level(self: &Self) -> Option<usize> {
        if self.arrow_params.len() > 0 || self.tail.len() > 0 {
            None
        } else {
            match self.head {
                Ident::Universe(l) => Some(l),
                _ => None,
            }
        }
    }

    fn insert(self: &mut Self, mut other: Self) {
        if (other.universe_level().is_some() || other.arrow_params.len() > 0)
            && self.tail.len() > 0
        {
            panic!("Substituted arrow expression into head position");
        }
        self.arrow_params.append(&mut other.arrow_params);
        self.head = other.head;
        // @Performance reverse all tail arrays
        // specifically to make this faster
        other.tail.append(&mut self.tail);
        self.tail = other.tail;
    }

    fn write_grouped(
        self: &Self,
        f: &mut std::fmt::Formatter,
        group_algs: bool,
    ) -> std::fmt::Result {
        if self.arrow_params.len() > 0 || (group_algs && self.tail.len() > 0) {
            write!(f, "({})", self)?;
        } else {
            write!(f, "{}", self)?;
        }
        Ok(())
    }
}

impl std::fmt::Display for Expr {
    fn fmt(self: &Self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for ex in &self.arrow_params {
            ex.write_grouped(f, false)?;
            write!(f, " -> ")?;
        }
        match self.head {
            Ident::Universe(l) => {
                write!(f, "U{}", l)?;
            },
            Ident::Local(i) => {
                write!(f, "x{}", i)?;
            },
            Ident::Global(i) => {
                write!(f, "g{}", i)?;
            },
        }
        for ex in &self.tail {
            write!(f, " ")?;
            ex.write_grouped(f, true)?;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Context<'a, T> {
    prev_size: usize,
    prev: Option<&'a Context<'a, T>>,
    this: &'a [T],
}

impl<'a, T> Context<'a, T> {
    fn new(this: &'a [T]) -> Self {
        Context { this, prev_size: 0, prev: None }
    }
    fn push(self: &'a Self, next: &'a [T]) -> Self {
        // this doesn't actually shadow anything because of the size we use
        self.push_shadowed(next, self.size())
    }
    // shadows indeces, cannot be used with shadowed names
    // (maybe I should stop calling one of these shadowing...)
    fn push_shadowed(self: &'a Self, next: &'a [T], unshadowed: usize) -> Self {
        Context {
            prev_size: unshadowed,
            prev: Some(self),
            this: next,
        }
    }
    fn size(self: &Self) -> usize {
        self.prev_size + self.this.len()
    }

    // NOT valid on expressions with shadowed indeces
    // this is so that we can efficiently implement shadowed _parameter names_
    fn index_from_value(self: &Self, name: &T) -> Option<usize>
        where T: PartialEq,
    {
        let mut curr = self;
        loop {
            if let Some(result) = get_index(&curr.this, name) {
                return Some(result + curr.prev_size);
            }
            if let Some(prev) = &curr.prev {
                curr = prev;
            } else {
                return None;
            }
        }
    }
    fn value_from_index(self: &'a Self, index: usize) -> &'a T {
        let mut curr = self;
        loop {
            if index < curr.prev_size {
                curr = curr.prev.expect("Nonzero prev_size but no prev??");
            } else {
                return &curr.this[index - curr.prev_size];
            }
        }
    }
}

fn get_index<T: PartialEq>(names: &[T], name: &T) -> Option<usize> {
    let mut result = names.len();
    while result > 0 {
        result -= 1;
        if names[result] == *name {
            return Some(result);
        }
    }
    None
}

fn convert_expr(
    globals: &Vec<String>,
    locals: &Context<String>,
    mut expr: ast::Expr,
) -> Expr {
    let mut arrow_params = Vec::new();
    let mut new_locals = Vec::new();
    while let ast::Expr::Arrow(ast::ArrowExpr { params, output }) = expr {
        for (name, ty) in params {
            arrow_params.push(
                convert_expr(globals, &locals.push(&new_locals), ty)
            );
            new_locals.push(name.unwrap_or_else(|| "_".into()));
        }
        expr = *output;
    }
    let locals = locals.push(&new_locals);
    let alg = match expr {
        ast::Expr::Arrow(_) => unreachable!(),
        ast::Expr::Alg(alg) => alg,
    };

    let head = {
        if let Some(id) = locals.index_from_value(&alg.head) {
            Ident::Local(id)
        } else if let Some(id) = get_index(globals, &alg.head) {
            Ident::Global(id)
        } else {
            if &alg.head[..1] != "U" {
                panic!("Could not find term for identifier: {}", alg.head);
            }
            if let Ok(l) = alg.head[1..].parse() {
                Ident::Universe(l)
            } else {
                panic!("Could not find term for identifier: {}", alg.head);
            }
        }
    };
    let tail = alg
        .tail
        .into_iter()
        .map(|ex| convert_expr(globals, &locals, ex))
        .collect();
    Expr { arrow_params, head, tail }
}

fn calculate_type(
    globals: &Vec<Item>,
    locals: &Context<Expr>,
    expr: &Expr,
) -> Expr {
    // process variables introduced by arrow expressions
    let mut new_locals = Vec::new();
    for each in &expr.arrow_params {
        sort_check_expr(
            globals,
            &locals.push(&new_locals),
            each,
        );
        new_locals.push(each.clone());
    }
    let locals = locals.push(&new_locals);
    // initialize with type of term in head position
    let (mut actual, mut expr_ctx_size) = match expr.head {
        Ident::Local(i) => (locals.value_from_index(i).clone(), i),
        Ident::Global(i) => (globals[i].ty.clone(), 0),
        Ident::Universe(l) => {
            if expr.tail.len() > 0 {
                panic!("Cannot apply type to arguments");
            }
            return Expr::universe(l+1);
        },
    };
    // check that arguments match the type expected in head position
    let mut checked = 0;
    let mut subbed = 0;
    while checked < expr.tail.len() {
        if actual.arrow_params.len() == 0 {
            // @Performance lazy eval? save the full eval for later
            actual = subst(
                &actual, expr_ctx_size, 0,
                &expr.tail[subbed..checked], locals.size(),
            );
            subbed = checked;
            expr_ctx_size = locals.size();
            eval(globals, &mut actual, locals.size());
            if actual.arrow_params.len() == 0 {
                panic!("Cannot apply type family to argument(s): {}",
                       actual);
            }
        }
        // the first parameter of the actual type is the expected type for
        // the first argument
        let arg_expected_base = actual.arrow_params.remove(0);

        // @Memory maybe subst could take &mut param?
        // @Performance skip this cloning operation if i is 0?
        let mut arg_expected = subst(
            &arg_expected_base, expr_ctx_size, 0,
            &expr.tail[subbed..checked], locals.size(),
        );
        // @Performance that's a lot of eval
        eval(globals, &mut arg_expected, locals.size());
        type_check_expr(
            globals,
            &locals,
            &expr.tail[checked],
            &arg_expected,
        );
        checked += 1;
    }

    // return result of applying head to all given arguments
    actual = subst(
        &actual, expr_ctx_size, 0,
        &expr.tail[subbed..checked], locals.size(),
    );
    eval(globals, &mut actual, locals.size());
    if expr.arrow_params.len() > 0 && actual.universe_level().is_none() {
        panic!("Expected element of a universe (in result of arrow expression)");
    }
    return actual;
}

fn sort_check_expr(
    globals: &Vec<Item>,
    locals: &Context<Expr>,
    expr: &Expr,
) -> usize {
    let actual = calculate_type(globals, locals, expr);
    if let Some(l) = actual.universe_level() {
        l
    } else {
        panic!("Expected element of a universe");
    }
}

fn type_check_expr(
    globals: &Vec<Item>,
    locals: &Context<Expr>,
    expr: &Expr,
    expected: &Expr,
) {
    let actual = calculate_type(globals, locals, expr);
    assert_type(expr, &actual, expected);
}


fn assert_type(expr: &Expr, actual: &Expr, expected: &Expr) {
    if actual != expected {
        panic!("\n\n{} has type:\n  {}\n\nbut it was expected to have type:\n  {}\n\n",
               expr, actual, expected);
    }
}

fn eval_on(globals: &Vec<Item>, xs: &mut Vec<Expr>, ctx_size: &mut usize, incr: bool) {
    for x in xs {
        eval(globals, x, *ctx_size);
        if incr {
            *ctx_size += 1;
        }
    }
}

fn eval(globals: &Vec<Item>, expr: &mut Expr, mut ctx_size: usize) {
    eval_on(globals, &mut expr.arrow_params, &mut ctx_size, true);
    eval_on(globals, &mut expr.tail, &mut ctx_size, false);

    while let Ident::Global(i) = expr.head {
        if globals[i].def.is_none() {
            break;
        }
        let &(param_num, ref def) = globals[i].def.as_ref().unwrap();
        if expr.tail.len() < param_num {
            break;
        }

        let mut result = subst(
            &def, 0, 0,
            &expr.tail[0..param_num], ctx_size,
        );
        // recurse... often redundant... @Performance? combine with subst?
        // type checking should prevent associativity problems
        // A -> (B -> x y z) w
        eval_on(globals, &mut result.arrow_params, &mut ctx_size, true);
        eval_on(globals, &mut result.tail, &mut ctx_size, false);
        // @Performance we are allocating again every time...
        // could just combine these steps or something more tricky
        expr.tail.drain(0..param_num);
        expr.insert(result);
    }
}

// takes an expression M valid in G1, (s + m + e variables)
// and a set of arguments X1..Xm valid in G2 (n variables) where s <= n
// then generates an expression M[x(s+i) <- Xi, x(s+m+i) <- x(n+i)]
// but with Xi[x(n+i) <- x(n+e+i)] in each substitution,
// in cases where arrow expressions are substituted _into_ arrow expressions
fn subst(
    base: &Expr, shared_ctx_size: usize, mut extra_ctx_size: usize,
    args: &[Expr], arg_ctx_size: usize,
) -> Expr {
    // just a dumb default... we overwrite everything
    let mut result = Expr::universe(0);
    result.arrow_params = Vec::with_capacity(base.arrow_params.len());
    for ex in &base.arrow_params {
        result.arrow_params.push(
             subst(ex, shared_ctx_size, extra_ctx_size, args, arg_ctx_size)
        );
        extra_ctx_size += 1;
    }
    result.tail = Vec::with_capacity(base.tail.len());
    for ex in &base.tail {
        result.tail.push(
             subst(ex, shared_ctx_size, extra_ctx_size, args, arg_ctx_size)
        );
    }
    match base.head {
        Ident::Local(i) => {
            if i < shared_ctx_size {
                result.head =  Ident::Local(i);
            } else if i - shared_ctx_size < args.len() {
                let arg = deepen(
                    &args[i - shared_ctx_size],
                    arg_ctx_size,
                    extra_ctx_size,
                );
                result.insert(arg);
            } else {
                let e = i - (shared_ctx_size + args.len());
                result.head = Ident::Local(arg_ctx_size + e);
            }
        },
        _ => result.head = base.head,
    }
    result
}

fn deepen(arg: &Expr, arg_ctx_size: usize, extra: usize) -> Expr {
    let mut arrow_params = Vec::with_capacity(arg.arrow_params.len());
    for ex in &arg.arrow_params {
        arrow_params.push(
            deepen(ex, arg_ctx_size, extra)
        );
    }
    let mut tail = Vec::with_capacity(arg.tail.len());
    for ex in &arg.tail {
        tail.push(
            deepen(ex, arg_ctx_size, extra)
        );
    }
    let mut head = arg.head;
    if let Ident::Local(i) = &mut head {
        if *i >= arg_ctx_size {
            *i += extra;
        }
    }
    Expr { arrow_params, head, tail }
}

