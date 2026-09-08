application_id = 'com.dmfigueroa.ComicName'

fixed_size_dir = 'hicolor' / '512x512' / 'apps'
install_data(
  fixed_size_dir / ('@0@.png').format(application_id),
  install_dir: get_option('datadir') / 'icons' / fixed_size_dir
)

symbolic_dir = 'hicolor' / 'symbolic' / 'apps'
install_data(
  symbolic_dir / ('@0@-symbolic.svg').format(application_id),
  install_dir: get_option('datadir') / 'icons' / symbolic_dir
)
