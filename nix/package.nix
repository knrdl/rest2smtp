{
  pkgs ? import <nixpkgs> { },
  lib ? pkgs.lib,
}:
let
  version = "0.0.0";
  cargoHash = "sha256-HHTOkjsJ46t3BtumtLi2yX1zFpk5G07ualhHeQWzVaU=";
  swaggerUiRev = "v5.18.2";
  swaggerUiHash = "sha256-JceFGTjNicDUVPanDPk5TUDeG0oFWyzC8SCFXbOPC1o=";

  src = lib.cleanSource ./..;

  swaggerUi = pkgs.fetchFromGitHub {
    owner = "swagger-api";
    repo = "swagger-ui";
    rev = swaggerUiRev;
    hash = swaggerUiHash;
  };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "rest2smtp";
  inherit version src;
  inherit cargoHash;

  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.openssl ];
  OPENSSL_NO_VENDOR = 1;

  postInstall = ''
    mkdir -p "$out/share/rest2smtp/www"
    cp -r "${swaggerUi}/dist/." "$out/share/rest2smtp/www/"
    for f in "$src/www"/*; do
      install -m0644 "$f" "$out/share/rest2smtp/www/$(basename "$f")"
    done
    install -m0644 "$src/Rocket.toml" "$out/share/rest2smtp/"
  '';

  meta = with lib; {
    description = "Send mails via REST API";
    homepage = "https://github.com/knrdl/rest2smtp";
    license = licenses.mit;
    mainProgram = "rest2smtp";
  };
}
