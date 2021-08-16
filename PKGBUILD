# Maintainer: Jamie Quigley <jamie@quigley.xyz>
pkgname=videoconverter-git
pkgver=e9c854d
pkgrel=1
pkgdesc="A program to convert video files"
arch=('i686' 'x86_64')
url="https://github.com/Sciencentisguy/videoconverter"
license=('GPLv3')
depends=('ffmpeg' 'libfdk-aac')
makedepends=('cargo' 'git')
source=("$pkgname::git+https://github.com/Sciencentistguy/videoconverter.git")
sha1sums=('SKIP')

pkgver() {
  cd "$pkgname"
  printf "r%s.%s" "$(git rev-list --count HEAD)" "$(git rev-parse --short HEAD)"
}

build() {
    cd "$pkgname"
    cargo build --release
}

package() {
    cd "$pkgname"
    install -Dm755 "target/release/videoconverter" "$pkgdir/usr/bin/videoconverter"

    install -Dm644 "LICENCE" "$pkgdir/usr/share/licenses/${pkgname}/LICENCE"
}
