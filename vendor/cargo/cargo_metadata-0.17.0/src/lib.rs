#![deny(missing_docs)]
//! Structured access to the output of `cargo metadata` and `cargo --message-format=json`.
//! Usually used from within a `cargo-*` executable
//!
//! See the [cargo book](https://doc.rust-lang.org/cargo/index.html) for
//! details on cargo itself.
//!
//! ## Examples
//!
//! ```rust
//! # extern crate cargo_metadata;
//! # use std::path::Path;
//! let mut args = std::env::args().skip_while(|val| !val.starts_with("--manifest-path"));
//!
//! let mut cmd = cargo_metadata::MetadataCommand::new();
//! let manifest_path = match args.next() {
//!     Some(ref p) if p == "--manifest-path" => {
//!         cmd.manifest_path(args.next().unwrap());
//!     }
//!     Some(p) => {
//!         cmd.manifest_path(p.trim_start_matches("--manifest-path="));
//!     }
//!     None => {}
//! };
//!
//! let _metadata = cmd.exec().unwrap();
//! ```
//!
//! Pass features flags
//!
//! ```rust
//! # // This should be kept in sync with the equivalent example in the readme.
//! # extern crate cargo_metadata;
//! # use std::path::Path;
//! # fn main() {
//! use cargo_metadata::{MetadataCommand, CargoOpt};
//!
//! let _metadata = MetadataCommand::new()
//!     .manifest_path("./Cargo.toml")
//!     .features(CargoOpt::AllFeatures)
//!     .exec()
//!     .unwrap();
//! # }
//! ```
//!
//! Parse message-format output:
//!
//! ```
//! # extern crate cargo_metadata;
//! use std::process::{Stdio, Command};
//! use cargo_metadata::Message;
//!
//! let mut command = Command::new("cargo")
//!     .args(&["build", "--message-format=json-render-diagnostics"])
//!     .stdout(Stdio::piped())
//!     .spawn()
//!     .unwrap();
//!
//! let reader = std::io::BufReader::new(command.stdout.take().unwrap());
//! for message in cargo_metadata::Message::parse_stream(reader) {
//!     match message.unwrap() {
//!         Message::CompilerMessage(msg) => {
//!             println!("{:?}", msg);
//!         },
//!         Message::CompilerArtifact(artifact) => {
//!             println!("{:?}", artifact);
//!         },
//!         Message::BuildScriptExecuted(script) => {
//!             println!("{:?}", script);
//!         },
//!         Message::BuildFinished(finished) => {
//!             println!("{:?}", finished);
//!         },
//!         _ => () // Unknown message
//!     }
//! }
//!
//! let output = command.wait().expect("Couldn't get cargo's exit status");
//! ```

use camino::Utf8PathBuf;
#[cfg(feature = "builder")]
use derive_builder::Builder;
use std::collections::BTreeMap;
use std::env;
use std::ffi::OsString;
use std::fmt;
use std::hash::Hash;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::str::from_utf8;

pub use camino;
pub use semver;
use semver::Version;

#[cfg(feature = "builder")]
pub use dependency::DependencyBuilder;
pub use dependency::{Dependency, DependencyKind};
use diagnostic::Diagnostic;
pub use errors::{Error, Result};
#[allow(deprecated)]
pub use messages::parse_messages;
pub use messages::{
    Artifact, ArtifactDebuginfo, ArtifactProfile, BuildFinished, BuildScript, CompilerMessage,
    Message, MessageIter,
};
#[cfg(feature = "builder")]
pub use messages::{
    ArtifactBuilder, ArtifactProfileBuilder, BuildFinishedBuilder, BuildScriptBuilder,
    CompilerMessageBuilder,
};
use serde::{Deserialize, Deserializer, Serialize};

mod dependency;
pub mod diagnostic;
mod errors;
mod messages;

/// An "opaque" identifier for a package.
///
/// It is possible to inspect the `repr` field, if the need arises, but its
/// precise format is an implementation detail and is subject to change.
///
/// `Metadata` can be indexed by `PackageId`.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct PackageId {
    /// The underlying string representation of id.
    pub repr: String,
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.repr, f)
    }
}

/// Helpers for default metadata fields
fn is_null(value: &serde_json::Value) -> bool {
    matches!(value, serde_json::Value::Null)
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[non_exhaustive]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
/// Starting point for metadata returned by `cargo metadata`
pub struct Metadata {
    /// A list of all crates referenced by this crate (and the crate itself)
    pub packages: Vec<Package>,
    /// A list of all workspace members
    pub workspace_members: Vec<PackageId>,
    /// Dependencies graph
    pub resolve: Option<Resolve>,
    /// Workspace root
    pub workspace_root: Utf8PathBuf,
    /// Build directory
    pub target_directory: Utf8PathBuf,
    /// The workspace-level metadata object. Null if non-existent.
    #[serde(rename = "metadata", default, skip_serializing_if = "is_null")]
    pub workspace_metadata: serde_json::Value,
    /// The metadata format version
    version: usize,
}

impl Metadata {
    /// Get the workspace's root package of this metadata instance.
    pub fn root_package(&self) -> Option<&Package> {
        match &self.resolve {
            Some(resolve) => {
                // if dependencies are resolved, use Cargo's answer
                let root = resolve.root.as_ref()?;
                self.packages.iter().find(|pkg| &pkg.id == root)
            }
            None => {
                // if dependencies aren't resolved, check for a root package manually
                let root_manifest_path = self.workspace_root.join("Cargo.toml");
                self.packages
                    .iter()
                    .find(|pkg| pkg.manifest_path == root_manifest_path)
            }
        }
    }

    /// Get the workspace packages.
    pub fn workspace_packages(&self) -> Vec<&Package> {
        self.packages
            .iter()
            .filter(|&p| self.workspace_members.contains(&p.id))
            .collect()
    }
}

impl<'a> std::ops::Index<&'a PackageId> for Metadata {
    type Output = Package;

    fn index(&self, idx: &'a PackageId) -> &Package {
        self.packages
            .iter()
            .find(|p| p.id == *idx)
            .unwrap_or_else(|| panic!("no package with this id: {:?}", idx))
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[non_exhaustive]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
/// A dependency graph
pub struct Resolve {
    /// Nodes in a dependencies graph
    pub nodes: Vec<Node>,

    /// The crate for which the metadata was read.
    pub root: Option<PackageId>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[non_exhaustive]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
/// A node in a dependencies graph
pub struct Node {
    /// An opaque identifier for a package
    pub id: PackageId,
    /// Dependencies in a structured format.
    ///
    /// `deps` handles renamed dependencies whereas `dependencies` does not.
    #[serde(default)]
    pub deps: Vec<NodeDep>,

    /// List of opaque identifiers for this node's dependencies.
    /// It doesn't support renamed dependencies. See `deps`.
    pub dependencies: Vec<PackageId>,

    /// Features enabled on the crate
    #[serde(default)]
    pub features: Vec<String>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[non_exhaustive]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
/// A dependency in a node
pub struct NodeDep {
    /// The name of the dependency's library target.
    /// If the crate was renamed, it is the new name.
    pub name: String,
    /// Package ID (opaque unique identifier)
    pub pkg: PackageId,
    /// The kinds of dependencies.
    ///
    /// This field was added in Rust 1.41.
    #[serde(default)]
    pub dep_kinds: Vec<DepKindInfo>,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[non_exhaustive]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
/// Information about a dependency kind.
pub struct DepKindInfo {
    /// The kind of dependency.
    #[serde(deserialize_with = "dependency::parse_dependency_kind")]
    pub kind: DependencyKind,
    /// The target platform for the dependency.
    ///
    /// This is `None` if it is not a target dependency.
    ///
    /// Use the [`Display`] trait to access the contents.
    ///
    /// By default all platform dependencies are included in the resolve
    /// graph. Use Cargo's `--filter-platform` flag if you only want to
    /// include dependencies for a specific platform.
    ///
    /// [`Display`]: std::fmt::Display
    pub target: Option<dependency::Platform>,
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[non_exhaustive]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
/// One or more crates described by a single `Cargo.toml`
///
/// Each [`target`][Package::targets] of a `Package` will be built as a crate.
/// For more information, see <https://doc.rust-lang.org/book/ch07-01-packages-and-crates.html>.
pub struct Package {
    /// Name as given in the `Cargo.toml`
    pub name: String,
    /// Version given in the `Cargo.toml`
    pub version: Version,
    /// Authors given in the `Cargo.toml`
    #[serde(default)]
    pub authors: Vec<String>,
    /// An opaque identifier for a package
    pub id: PackageId,
    /// The source of the package, e.g.
    /// crates.io or `None` for local projects.
    pub source: Option<Source>,
    /// Description as given in the `Cargo.toml`
    pub description: Option<String>,
    /// List of dependencies of this particular package
    pub dependencies: Vec<Dependency>,
    /// License as given in the `Cargo.toml`
    pub license: Option<String>,
    /// If the package is using a nonstandard license, this key may be specified instead of
    /// `license`, and must point to a file relative to the manifest.
    pub license_file: Option<Utf8PathBuf>,
    /// Targets provided by the crate (lib, bin, example, test, ...)
    pub targets: Vec<Target>,
    /// Features provided by the crate, mapped to the features required by that feature.
    pub features: BTreeMap<String, Vec<String>>,
    /// Path containing the `Cargo.toml`
    pub manifest_path: Utf8PathBuf,
    /// Categories as given in the `Cargo.toml`
    #[serde(default)]
    pub categories: Vec<String>,
    /// Keywords as given in the `Cargo.toml`
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Readme as given in the `Cargo.toml`
    pub readme: Option<Utf8PathBuf>,
    /// Repository as given in the `Cargo.toml`
    // can't use `url::Url` because that requires a more recent stable compiler
    pub repository: Option<String>,
    /// Homepage as given in the `Cargo.toml`
    ///
    /// On versions of cargo before 1.49, this will always be [`None`].
    pub homepage: Option<String>,
    /// Documentation URL as given in the `Cargo.toml`
    ///
    /// On versions of cargo before 1.49, this will always be [`None`].
    pub documentation: Option<String>,
    /// Default Rust edition for the package
    ///
    /// Beware that individual targets may specify their own edition in
    /// [`Target::edition`].
    #[serde(default)]
    pub edition: Edition,
    /// Contents of the free form package.metadata section
    ///
    /// This contents can be serialized to a struct using serde:
    ///
    /// ```rust
    /// use serde::Deserialize;
    /// use serde_json::json;
    ///
    /// #[derive(Debug, Deserialize)]
    /// struct SomePackageMetadata {
    ///     some_value: i32,
    /// }
    ///
    /// fn main() {
    ///     let value = json!({
    ///         "some_value": 42,
    ///     });
    ///
    ///     let package_metadata: SomePackageMetadata = serde_json::from_value(value).unwrap();
    ///     assert_eq!(package_metadata.some_value, 42);
    /// }
    ///
    /// ```
    #[serde(default, skip_serializing_if = "is_null")]
    pub metadata: serde_json::Value,
    /// The name of a native library the package is linking to.
    pub links: Option<String>,
    /// List of registries to which this package may be published.
    ///
    /// Publishing is unrestricted if `None`, and forbidden if the `Vec` is empty.
    ///
    /// This is always `None` if running with a version of Cargo older than 1.39.
    pub publish: Option<Vec<String>>,
    /// The default binary to run by `cargo run`.
    ///
    /// This is always `None` if running with a version of Cargo older than 1.55.
    pub default_run: Option<String>,
    /// The minimum supported Rust version of this package.
    ///
    /// This is always `None` if running with a version of Cargo older than 1.58.
    #[serde(default)]
    #[serde(deserialize_with = "deserialize_rust_version")]
    pub rust_version: Option<Version>,
}

impl Package {
    /// Full path to the license file if one is present in the manifest
    pub fn license_file(&self) -> Option<Utf8PathBuf> {
        self.license_file.as_ref().map(|file| {
            self.manifest_path
                .parent()
                .unwrap_or(&self.manifest_path)
                .join(file)
        })
    }

    /// Full path to the readme file if one is present in the manifest
    pub fn readme(&self) -> Option<Utf8PathBuf> {
        self.readme.as_ref().map(|file| {
            self.manifest_path
                .parent()
                .unwrap_or(&self.manifest_path)
                .join(file)
        })
    }
}

/// The source of a package such as crates.io.
///
/// It is possible to inspect the `repr` field, if the need arises, but its
/// precise format is an implementation detail and is subject to change.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(transparent)]
pub struct Source {
    /// The underlying string representation of a source.
    pub repr: String,
}

impl Source {
    /// Returns true if the source is crates.io.
    pub fn is_crates_io(&self) -> bool {
        self.repr == "registry+https://github.com/rust-lang/crates.io-index"
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.repr, f)
    }
}

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "builder", derive(Builder))]
#[cfg_attr(feature = "builder", builder(pattern = "owned", setter(into)))]
#[non_exhaustive]
/// A single target (lib, bin, example, ...) provided by a crate
pub struct Target {
    /// Name as given in the `Cargo.toml` or generated from the file name
    pub name: String,
    /// Kind of target ("bin", "example", "test", "bench", "lib", "custom-build")
    pub kind: Vec<String>,
    /// Almost the same as `kind`, except when an example is a library instead of an executable.
    /// In that case `crate_types` contains things like `rlib` and `dylib` while `kind` is `example`
    #[serde(default)]
    #[cfg_attr(feature = "builder", builder(default))]
    pub crate_types: Vec<String>,

    #[serde(default)]
    #[cfg_attr(feature = "builder", builder(default))]
    #[serde(rename = "required-features")]
    /// This target is built only if these features are enabled.
    /// It doesn't apply to `lib` targets.
    pub required_features: Vec<String>,
    /// Path to the main source file of the target
    pub src_path: Utf8PathBuf,
    /// Rust edition for this target
    #[serde(default)]
    #[cfg_attr(feature = "builder", builder(default))]
    pub edition: Edition,
    /// Whether or not this target has doc tests enabled, and the target is
    /// compatible with doc testing.
    ///
    /// This is always `true` if running with a version of Cargo older than 1.37.
    #[serde(default = "default_true")]
    #[cfg_attr(feature = "builder", builder(default = "true"))]
    pub doctest: bool,
    /// Whether or not this target is tested by default by `cargo test`.
    ///
    /// This is always `true` if running with a version of Cargo older than 1.47.
    #[serde(default = "default_true")]
    #[cfg_attr(feature = "builder", builder(default = "true"))]
    pub test: bool,
    /// Whether or not this target is documented by `cargo doc`.
    ///
    /// This is always `true` if running with a version of Cargo older than 1.50.
    #[serde(default = "default_true")]
    #[cfg_attr(feature = "builder", builder(default = "true"))]
    pub doc: bool,
}

impl Target {
    fn is_kind(&self, name: &str) -> bool {
        self.kind.iter().any(|kind| kind == name)
    }

    /// Return true if this target is of kind "lib".
    pub fn is_lib(&self) -> bool {
        self.is_kind("lib")
    }

    /// Return true if this target is of kind "bin".
    pub fn is_bin(&self) -> bool {
        self.is_kind("bin")
    }

    /// Return true if this target is of kind "example".
    pub fn is_example(&self) -> bool {
        self.is_kind("example")
    }

    /// Return true if this target is of kind "test".
    pub fn is_test(&self) -> bool {
        self.is_kind("test")
    }

    /// Return true if this target is of kind "bench".
    pub fn is_bench(&self) -> bool {
        self.is_kind("bench")
    }

    /// Return true if this target is of kind "custom-build".
    pub fn is_custom_build(&self) -> bool {
        self.is_kind("custom-build")
    }
}

/// The Rust edition
///
/// As of writing this comment rust editions 2024, 2027 and 2030 are not actually a thing yet but are parsed nonetheless for future proofing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum Edition {
    /// Edition 2015
    #[serde(rename = "2015")]
    E2015,
    /// Edition 2018
    #[serde(rename = "2018")]
    E2018,
    /// Edition 2021
    #[serde(rename = "2021")]
    E2021,
    #[doc(hidden)]
    #[serde(rename = "2024")]
    _E2024,
    #[doc(hidden)]
    #[serde(rename = "2027")]
    _E2027,
    #[doc(hidden)]
    #[serde(rename = "2030")]
    _E2030,
}

impl Edition {
    /// Return the string representation of the edition
    pub fn as_str(&self) -> &'static str {
        use Edition::*;
        match self {
            E2015 => "2015",
            E2018 => "2018",
            E2021 => "2021",
            _E2024 => "2024",
            _E2027 => "2027",
            _E2030 => "2030",
        }
    }
}

impl fmt::Display for Edition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Default for Edition {
    fn default() -> Self {
        Self::E2015
    }
}

fn default_true() -> bool {
    true
}

/// Cargo features flags
#[derive(Debug, Clone)]
pub enum CargoOpt {
    /// Run cargo with `--features-all`
    AllFeatures,
    /// Run cargo with `--no-default-features`
    NoDefaultFeatures,
    /// Run cargo with `--features <FEATURES>`
    SomeFeatures(Vec<String>),
}

/// A builder for configurating `cargo metadata` invocation.
#[derive(Debug, Clone, Default)]
pub struct MetadataCommand {
    /// Path to `cargo` executable.  If not set, this will use the
    /// the `$CARGO` environment variable, and if that is not set, will
    /// simply be `cargo`.
    cargo_path: Option<PathBuf>,
    /// Path to `Cargo.toml`
    manifest_path: Option<PathBuf>,
    /// Current directory of the `cargo metadata` process.
    current_dir: Option<PathBuf>,
    /// Output information only about workspace members and don't fetch dependencies.
    no_deps: bool,
    /// Collections of `CargoOpt::SomeFeatures(..)`
    features: Vec<String>,
    /// Latched `CargoOpt::AllFeatures`
    all_features: bool,
    /// Latched `CargoOpt::NoDefaultFeatures`
    no_default_features: bool,
    /// Arbitrary command line flags to pass to `cargo`.  These will be added
    /// to the end of the command line invocation.
    other_options: Vec<String>,
    /// Arbitrary environment variables to set when running `cargo`.  These will be merged into
    /// the calling environment, overriding any which clash.
    env: BTreeMap<OsString, OsString>,
    /// Show stderr
    verbose: bool,
}

impl MetadataCommand {
    /// Creates a default `cargo metadata` command, which will look for
    /// `Cargo.toml` in the ancestors of the current directory.
    pub fn new() -> MetadataCommand {
        MetadataCommand::default()
    }
    /// Path to `cargo` executable.  If not set, this will use the
    /// the `$CARGO` environment variable, and if that is not set, will
    /// simply be `cargo`.
    pub fn cargo_path(&mut self, path: impl Into<PathBuf>) -> &mut MetadataCommand {
        self.cargo_path = Some(path.into());
        self
    }
    /// Path to `Cargo.toml`
    pub fn manifest_path(&mut self, path: impl Into<PathBuf>) -> &mut MetadataCommand {
        self.manifest_path = Some(path.into());
        self
    }
    /// Current directory of the `cargo metadata` process.
    pub fn current_dir(&mut self, path: impl Into<PathBuf>) -> &mut MetadataCommand {
        self.current_dir = Some(path.into());
        self
    }
    /// Output information only about workspace members and don't fetch dependencies.
    pub fn no_deps(&mut self) -> &mut MetadataCommand {
        self.no_deps = true;
        self
    }
    /// Which features to include.
    ///
    /// Call this multiple times to specify advanced feature configurations:
    ///
    /// ```no_run
    /// # use cargo_metadata::{CargoOpt, MetadataCommand};
    /// MetadataCommand::new()
    ///     .features(CargoOpt::NoDefaultFeatures)
    ///     .features(CargoOpt::SomeFeatures(vec!["feat1".into(), "feat2".into()]))
    ///     .features(CargoOpt::SomeFeatures(vec!["feat3".into()]))
    ///     // ...
    ///     # ;
    /// ```
    ///
    /// # Panics
    ///
    /// `cargo metadata` rejects multiple `--no-default-features` flags. Similarly, the `features()`
    /// method panics when specifying multiple `CargoOpt::NoDefaultFeatures`:
    ///
    /// ```should_panic
    /// # use cargo_metadata::{CargoOpt, MetadataCommand};
    /// MetadataCommand::new()
    ///     .features(CargoOpt::NoDefaultFeatures)
    ///     .features(CargoOpt::NoDefaultFeatures) // <-- panic!
    ///     // ...
    ///     # ;
    /// ```
    ///
    /// The method also panics for multiple `CargoOpt::AllFeatures` arguments:
    ///
    /// ```should_panic
    /// # use cargo_metadata::{CargoOpt, MetadataCommand};
    /// MetadataCommand::new()
    ///     .features(CargoOpt::AllFeatures)
    ///     .features(CargoOpt::AllFeatures) // <-- panic!
    ///     // ...
    ///     # ;
    /// ```
    pub fn features(&mut self, features: CargoOpt) -> &mut MetadataCommand {
        match features {
            CargoOpt::SomeFeatures(features) => self.features.extend(features),
            CargoOpt::NoDefaultFeatures => {
                assert!(
                    !self.no_default_features,
                    "Do not supply CargoOpt::NoDefaultFeatures more than once!"
                );
                self.no_default_features = true;
            }
            CargoOpt::AllFeatures => {
                assert!(
                    !self.all_features,
                    "Do not supply CargoOpt::AllFeatures more than once!"
                );
                self.all_features = true;
            }
        }
        self
    }
    /// Arbitrary command line flags to pass to `cargo`.  These will be added
    /// to the end of the command line invocation.
    pub fn other_options(&mut self, options: impl Into<Vec<String>>) -> &mut MetadataCommand {
        self.other_options = options.into();
        self
    }

    /// Arbitrary environment variables to set when running `cargo`.  These will be merged into
    /// the calling environment, overriding any which clash.
    ///
    /// Some examples of when you may want to use this:
    /// 1. Setting cargo config values without needing a .cargo/config.toml file, e.g. to set
    ///    `CARGO_NET_GIT_FETCH_WITH_CLI=true`
    /// 2. To specify a custom path to RUSTC if your rust toolchain components aren't laid out in
    ///    the way cargo expects by default.
    ///
    /// ```no_run
    /// # use cargo_metadata::{CargoOpt, MetadataCommand};
    /// MetadataCommand::new()
    ///     .env("CARGO_NET_GIT_FETCH_WITH_CLI", "true")
    ///     .env("RUSTC", "/path/to/rustc")
    ///     // ...
    ///     # ;
    /// ```
    pub fn env<K: Into<OsString>, V: Into<OsString>>(
        &mut self,
        key: K,
        val: V,
    ) -> &mut MetadataCommand {
        self.env.insert(key.into(), val.into());
        self
    }

    /// Set whether to show stderr
    pub fn verbose(&mut self, verbose: bool) -> &mut MetadataCommand {
        self.verbose = verbose;
        self
    }

    /// Builds a command for `cargo metadata`.  This is the first
    /// part of the work of `exec`.
    pub fn cargo_command(&self) -> Command {
        let cargo = self
            .cargo_path
            .clone()
            .or_else(|| env::var("CARGO").map(PathBuf::from).ok())
            .unwrap_or_else(|| PathBuf::from("cargo"));
        let mut cmd = Command::new(cargo);
        cmd.args(["metadata", "--format-version", "1"]);

        if self.no_deps {
            cmd.arg("--no-deps");
        }

        if let Some(path) = self.current_dir.as_ref() {
            cmd.current_dir(path);
        }

        if !self.features.is_empty() {
            cmd.arg("--features").arg(self.features.join(","));
        }
        if self.all_features {
            cmd.arg("--all-features");
        }
        if self.no_default_features {
            cmd.arg("--no-default-features");
        }

        if let Some(manifest_path) = &self.manifest_path {
            cmd.arg("--manifest-path").arg(manifest_path.as_os_str());
        }
        cmd.args(&self.other_options);

        cmd.envs(&self.env);

        cmd
    }

    /// Parses `cargo metadata` output.  `data` must have been
    /// produced by a command built with `cargo_command`.
    pub fn parse<T: AsRef<str>>(data: T) -> Result<Metadata> {
        let meta = serde_json::from_str(data.as_ref())?;
        Ok(meta)
    }

    /// Runs configured `cargo metadata` and returns parsed `Metadata`.
    pub fn exec(&self) -> Result<Metadata> {
        let mut command = self.cargo_command();
        if self.verbose {
            command.stderr(Stdio::inherit());
        }
        let output = command.output()?;
        if !output.status.success() {
            return Err(Error::CargoMetadata {
                stderr: String::from_utf8(output.stderr)?,
            });
        }
        let stdout = from_utf8(&output.stdout)?
            .lines()
            .find(|line| line.starts_with('{'))
            .ok_or(Error::NoJson)?;
        Self::parse(stdout)
    }
}

/// As per the Cargo Book the [`rust-version` field](https://doc.rust-lang.org/cargo/reference/manifest.html#the-rust-version-field) must:
///
/// > be a bare version number with two or three components;
/// > it cannot include semver operators or pre-release identifiers.
///
/// [`semver::Version`] however requires three components. This function takes
/// care of appending `.0` if the provided version number only has two components
/// and ensuring that it does not contain a pre-release version or build metadata.
fn deserialize_rust_version<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<Version>, D::Error>
where
    D: Deserializer<'de>,
{
    let mut buf = match Option::<String>::deserialize(deserializer)? {
        None => return Ok(None),
        Some(buf) => buf,
    };

    for char in buf.chars() {
        if char == '-' {
            return Err(serde::de::Error::custom(
                "pre-release identifiers are not supported in rust-version",
            ));
        } else if char == '+' {
            return Err(serde::de::Error::custom(
                "build metadata is not supported in rust-version",
            ));
        }
    }

    if buf.matches('.').count() == 1 {
        // e.g. 1.0 -> 1.0.0
        buf.push_str(".0");
    }

    Ok(Some(
        Version::parse(&buf).map_err(serde::de::Error::custom)?,
    ))
}

#[cfg(test)]
mod test {
    use semver::Version;

    #[derive(Debug, serde::Deserialize)]
    struct BareVersion(
        #[serde(deserialize_with = "super::deserialize_rust_version")] Option<semver::Version>,
    );

    fn bare_version(str: &str) -> Version {
        serde_json::from_str::<BareVersion>(&format!(r#""{}""#, str))
            .unwrap()
            .0
            .unwrap()
    }

    fn bare_version_err(str: &str) -> String {
        serde_json::from_str::<BareVersion>(&format!(r#""{}""#, str))
            .unwrap_err()
            .to_string()
    }

    #[test]
    fn test_deserialize_rust_version() {
        assert_eq!(bare_version("1.2"), Version::new(1, 2, 0));
        assert_eq!(bare_version("1.2.0"), Version::new(1, 2, 0));
        assert_eq!(
            bare_version_err("1.2.0-alpha"),
            "pre-release identifiers are not supported in rust-version"
        );
        assert_eq!(
            bare_version_err("1.2.0+123"),
            "build metadata is not supported in rust-version"
        );
    }
}
