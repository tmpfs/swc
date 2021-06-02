use super::Bundler;
use crate::{load::Load, resolve::Resolve};
use anyhow::{Context, Error};
use fxhash::FxHashMap;
use fxhash::FxHashSet;
use retain_mut::RetainMut;
use swc_atoms::{js_word, JsWord};
use swc_common::{sync::Lrc, FileName, Mark, Spanned, SyntaxContext, DUMMY_SP};
use swc_ecma_ast::*;
use swc_ecma_utils::{find_ids, ident::IdentLike, Id};
use swc_ecma_visit::noop_visit_mut_type;
use swc_ecma_visit::VisitMut;
use swc_ecma_visit::VisitMutWith;

#[cfg(test)]
mod tests;

impl<L, R> Bundler<'_, L, R>
where
    L: Load,
    R: Resolve,
{
    /// This method de-globs imports if possible and colorizes imported values.
    pub(super) fn extract_import_info(
        &self,
        path: &FileName,
        module: &mut Module,
        module_local_mark: Mark,
    ) -> RawImports {
        self.run(|| {
            let mut v = ImportHandler {
                module_ctxt: SyntaxContext::empty().apply_mark(module_local_mark),
                path,
                bundler: self,
                top_level: false,
                info: Default::default(),
                usages: Default::default(),
                imported_idents: Default::default(),
                deglob_phase: false,
                idents_to_deglob: Default::default(),
                in_obj_of_member: false,
            };
            module.body.visit_mut_with(&mut v);
            v.deglob_phase = true;
            module.body.visit_mut_with(&mut v);

            v.info
        })
    }

    pub(super) fn resolve(
        &self,
        base: &FileName,
        module_specifier: &str,
    ) -> Result<Lrc<FileName>, Error> {
        self.run(|| {
            let path = self
                .resolver
                .resolve(base, module_specifier)
                .with_context(|| format!("failed to resolve {} from {}", module_specifier, base))?;

            let path = Lrc::new(path);

            Ok(path)
        })
    }
}
