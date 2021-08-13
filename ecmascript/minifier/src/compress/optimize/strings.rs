use super::Optimizer;
use swc_atoms::js_word;
use swc_common::Spanned;
use swc_ecma_ast::*;
use swc_ecma_transforms_base::ext::MapWithMut;
use swc_ecma_utils::{ident::IdentLike, ExprExt, Value::Known};

impl Optimizer<'_> {
    pub(super) fn optimize_expr_in_str_ctx_unsafely(&mut self, e: &mut Expr) {
        if !self.options.unsafe_passes {
            return;
        }

        match e {
            Expr::Call(CallExpr {
                callee: ExprOrSuper::Expr(callee),
                args,
                ..
            }) => {
                if args.iter().any(|arg| arg.expr.may_have_side_effects()) {
                    return;
                }

                match &**callee {
                    Expr::Ident(Ident {
                        sym: js_word!("RegExp"),
                        ..
                    }) if self.options.unsafe_regexp => {
                        if args.len() != 1 {
                            return;
                        }

                        self.optimize_expr_in_str_ctx(&mut args[0].expr);

                        match &*args[0].expr {
                            Expr::Lit(Lit::Str(..)) => {
                                self.changed = true;
                                log::debug!(
                                    "strings: Unsafely reduced `RegExp` call in a string context"
                                );

                                *e = *args[0].expr.take();
                                return;
                            }

                            _ => {}
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }

    /// Convert expressions to string literal if possible.
    pub(super) fn optimize_expr_in_str_ctx(&mut self, n: &mut Expr) {
        match n {
            Expr::Lit(Lit::Str(..)) => return,
            Expr::Paren(e) => {
                self.optimize_expr_in_str_ctx(&mut e.expr);
                match &*e.expr {
                    Expr::Lit(Lit::Str(..)) => {
                        *n = *e.expr.take();
                        self.changed = true;
                        log::debug!("string: Removed a paren in a string context");
                    }
                    _ => {}
                }

                return;
            }
            _ => {}
        }

        let span = n.span();
        let value = n.as_string();
        if let Known(value) = value {
            self.changed = true;
            log::debug!(
                "strings: Converted an expression into a string literal (in string context)"
            );
            *n = Expr::Lit(Lit::Str(Str {
                span,
                value: value.into(),
                has_escape: false,
                kind: Default::default(),
            }));
            return;
        }

        match n {
            Expr::Lit(Lit::Num(v)) => {
                self.changed = true;
                log::debug!(
                    "strings: Converted a numeric literal ({}) into a string literal (in string \
                     context)",
                    v.value
                );

                *n = Expr::Lit(Lit::Str(Str {
                    span: v.span,
                    value: format!("{:?}", v.value).into(),
                    has_escape: false,
                    kind: Default::default(),
                }));
                return;
            }

            Expr::Lit(Lit::Regex(v)) => {
                if !self.options.evaluate {
                    return;
                }
                self.changed = true;
                log::debug!(
                    "strings: Converted a regex (/{}/{}) into a string literal (in string context)",
                    v.exp,
                    v.flags
                );

                *n = Expr::Lit(Lit::Str(Str {
                    span: v.span,
                    value: format!("/{}/{}", v.exp, v.flags).into(),
                    has_escape: false,
                    kind: Default::default(),
                }));
                return;
            }

            Expr::Ident(i) => {
                if !self.options.evaluate || !self.options.reduce_vars {
                    return;
                }
                if self
                    .data
                    .as_ref()
                    .and_then(|data| data.vars.get(&i.to_id()))
                    .map(|v| v.assign_count == 0)
                    .unwrap_or(false)
                {
                    self.changed = true;
                    log::debug!(
                        "strings: Converting a reference ({}{:?}) into `undefined` (in string \
                         context)",
                        i.sym,
                        i.span.ctxt
                    );

                    *n = Expr::Lit(Lit::Str(Str {
                        span: i.span,
                        value: js_word!("undefined"),
                        has_escape: false,
                        kind: Default::default(),
                    }));
                }
            }

            _ => {}
        }
    }
}
