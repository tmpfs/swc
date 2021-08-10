//! Faster version of node-resolve.
//!
//! See: https://github.com/goto-bus-stop/node-resolve

use crate::resolve::Resolve;
use anyhow::{bail, Context, Error};
#[cfg(windows)]
use normpath::BasePath;
use serde::Deserialize;
use std::{
    fs::File,
    io::BufReader,
    path::{Component, Path, PathBuf},
};
use swc_common::FileName;
use swc_ecma_ast::TargetEnv;

// Run `node -p "require('module').builtinModules"`
pub(crate) fn is_core_module(s: &str) -> bool {
    match s {
        "_http_agent"
        | "_http_client"
        | "_http_common"
        | "_http_incoming"
        | "_http_outgoing"
        | "_http_server"
        | "_stream_duplex"
        | "_stream_passthrough"
        | "_stream_readable"
        | "_stream_transform"
        | "_stream_wrap"
        | "_stream_writable"
        | "_tls_common"
        | "_tls_wrap"
        | "assert"
        | "assert/strict"
        | "async_hooks"
        | "buffer"
        | "child_process"
        | "cluster"
        | "console"
        | "constants"
        | "crypto"
        | "dgram"
        | "diagnostics_channel"
        | "dns"
        | "dns/promises"
        | "domain"
        | "events"
        | "fs"
        | "fs/promises"
        | "http"
        | "http2"
        | "https"
        | "inspector"
        | "module"
        | "net"
        | "os"
        | "path"
        | "path/posix"
        | "path/win32"
        | "perf_hooks"
        | "process"
        | "punycode"
        | "querystring"
        | "readline"
        | "repl"
        | "stream"
        | "stream/promises"
        | "string_decoder"
        | "sys"
        | "timers"
        | "timers/promises"
        | "tls"
        | "trace_events"
        | "tty"
        | "url"
        | "util"
        | "util/types"
        | "v8"
        | "vm"
        | "wasi"
        | "worker_threads"
        | "zlib" => true,
        _ => false,
    }
}

#[derive(Deserialize)]
struct PackageJson {
    #[serde(default)]
    esnext: Option<String>,
    #[serde(default)]
    main: Option<String>,
    #[serde(default)]
    browser: Option<String>,
}

#[derive(Debug, Default)]
pub struct NodeModulesResolver {
    target_env: TargetEnv,
}

static EXTENSIONS: &[&str] = &["ts", "tsx", "js", "jsx", "json", "node"];

impl NodeModulesResolver {
    /// Create a node modules resolver for the target runtime environment.
    pub fn new(target_env: TargetEnv) -> Self {
        Self { target_env }
    }

    fn wrap(&self, path: PathBuf) -> Result<FileName, Error> {
        let path = path.canonicalize().context("failed to canonicalize")?;
        Ok(FileName::Real(path))
    }

    /// Resolve a path as a file. If `path` refers to a file, it is returned;
    /// otherwise the `path` + each extension is tried.
    fn resolve_as_file(&self, path: &Path) -> Result<PathBuf, Error> {
        // 1. If X is a file, load X as JavaScript text.
        if path.is_file() {
            return Ok(path.to_path_buf());
        }

        if let Some(name) = path.file_name() {
            let mut ext_path = path.to_path_buf();
            let name = name.to_string_lossy();
            for ext in EXTENSIONS {
                ext_path.set_file_name(format!("{}.{}", name, ext));
                if ext_path.is_file() {
                    return Ok(ext_path);
                }
            }
        }

        bail!("file not found: {}", path.display())
    }

    /// Resolve a path as a directory, using the "main" key from a package.json
    /// file if it exists, or resolving to the index.EXT file if it exists.
    fn resolve_as_directory(&self, path: &PathBuf) -> Result<PathBuf, Error> {
        // 1. If X/package.json is a file, use it.
        let pkg_path = path.join("package.json");
        if pkg_path.is_file() {
            let main = self.resolve_package_main(&pkg_path);
            if main.is_ok() {
                return main;
            }
        }

        // 2. LOAD_INDEX(X)
        self.resolve_index(path)
    }

    /// Resolve using the package.json "main" key.
    fn resolve_package_main(&self, pkg_path: &PathBuf) -> Result<PathBuf, Error> {
        let pkg_dir = pkg_path.parent().unwrap_or_else(|| Path::new("/"));
        let file = File::open(pkg_path)?;
        let reader = BufReader::new(file);
        let pkg: PackageJson =
            serde_json::from_reader(reader).context("failed to deserialize package.json")?;

        let main_fields = match self.target_env {
            TargetEnv::Node => {
                vec![&pkg.esnext, &pkg.main]
            }
            TargetEnv::Browser => {
                vec![&pkg.browser, &pkg.esnext, &pkg.main]
            }
        };

        for main in main_fields {
            if let Some(target) = main {
                let path = pkg_dir.join(target);
                return self
                    .resolve_as_file(&path)
                    .or_else(|_| self.resolve_as_directory(&path));
            }
        }

        bail!("package.json does not contain a \"main\" string")
    }

    /// Resolve a directory to its index.EXT.
    fn resolve_index(&self, path: &PathBuf) -> Result<PathBuf, Error> {
        // 1. If X/index.js is a file, load X/index.js as JavaScript text.
        // 2. If X/index.json is a file, parse X/index.json to a JavaScript object.
        // 3. If X/index.node is a file, load X/index.node as binary addon.
        for ext in EXTENSIONS {
            let ext_path = path.join(format!("index.{}", ext));
            if ext_path.is_file() {
                return Ok(ext_path);
            }
        }

        bail!("index not found: {}", path.display())
    }

    /// Resolve by walking up node_modules folders.
    fn resolve_node_modules(&self, base_dir: &Path, target: &str) -> Result<PathBuf, Error> {
        let node_modules = base_dir.join("node_modules");
        if node_modules.is_dir() {
            let path = node_modules.join(target);
            let result = self
                .resolve_as_file(&path)
                .or_else(|_| self.resolve_as_directory(&path));
            if result.is_ok() {
                return result;
            }
        }

        match base_dir.parent() {
            Some(parent) => self.resolve_node_modules(parent, target),
            None => bail!("not found"),
        }
    }
}

impl Resolve for NodeModulesResolver {
    fn resolve(&self, base: &FileName, target: &str) -> Result<FileName, Error> {
        if let TargetEnv::Node = self.target_env {
            if is_core_module(target) {
                return Ok(FileName::Custom(target.to_string()));
            }
        }

        let base = match base {
            FileName::Real(v) => v,
            _ => bail!("node-resolver supports only files"),
        };

        let target_path = Path::new(target);

        if target_path.is_absolute() {
            let path = PathBuf::from(target_path);
            return self
                .resolve_as_file(&path)
                .or_else(|_| self.resolve_as_directory(&path))
                .and_then(|p| self.wrap(p));
        }

        let cwd = &Path::new(".");
        let base_dir = base.parent().unwrap_or(&cwd);
        let mut components = target_path.components();

        if let Some(Component::CurDir | Component::ParentDir) = components.next() {
            #[cfg(windows)]
            let path = {
                let base_dir = BasePath::new(base_dir).unwrap();
                base_dir
                    .join(target.replace('/', "\\"))
                    .normalize_virtually()
                    .unwrap()
                    .into_path_buf()
            };
            #[cfg(not(windows))]
            let path = base_dir.join(target);
            return self
                .resolve_as_file(&path)
                .or_else(|_| self.resolve_as_directory(&path))
                .and_then(|p| self.wrap(p));
        }

        self.resolve_node_modules(base_dir, target)
            .and_then(|p| self.wrap(p))
    }
}
