pkgdatadir = get_option('prefix') / get_option('datadir') / meson.project_name()
gnome = import('gnome')

gnome.compile_resources('comic-name',
  'comic-name.gresource.xml',
  gresource_bundle: true,
  install: true,
  install_dir: pkgdatadir,
)

conf = configuration_data()
conf.set_quoted('VERSION', meson.project_version())
conf.set_quoted('GETTEXT_PACKAGE', 'comic-name')
conf.set_quoted('LOCALEDIR', get_option('prefix') / get_option('localedir'))
conf.set_quoted('PKGDATADIR', pkgdatadir)

configure_file(
  input: 'config.rs.in',
  output: 'config.rs',
  configuration: conf,
)

# Cargo currently consumes config.rs from the source directory.
run_command(
  'cp',
  meson.project_build_root() / 'src' / 'config.rs',
  meson.project_source_root() / 'src' / 'config.rs',
  check: true,
)

cargo_bin = find_program('cargo')
cargo_script = files('cargo-build.sh')
rust_target = 'debug'
if get_option('buildtype') == 'release'
  rust_target = 'release'
endif

custom_target(
  'cargo-build',
  build_by_default: true,
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: get_option('bindir'),
  command: [
    'sh',
    cargo_script,
    cargo_bin,
    meson.project_source_root() / 'Cargo.toml',
    meson.project_build_root() / 'src',
    rust_target,
    '@OUTPUT@',
  ],
)
