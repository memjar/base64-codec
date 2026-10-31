# TODO: move this to nixpkgs
# This file aims to be a replacement for the nixpkgs derivation.

{
  buildFeatures ? [ ],
  buildNoDefaultFeatures ? false,
  buildPackages,
  fetchFromGitHub,
  lib,
  rustPlatform,
}:

rustPlatform.buildRustPackage {
  inherit buildNoDefaultFeatures buildFeatures;

  pname = "base64-codec";
  version = "0.0.1";
  cargoHash = "";

  src = fetchFromGitHub {
    owner = "pimalaya";
    repo = "base64-codec";
    rev = "v0.0.1";
    hash = "";
  };

  meta = {
    description = "Base64 encoder and decoder with SIMD, no_std and dependency-free";
    homepage = "https://github.com/pimalaya/base64-codec";
    changelog = "https://github.com/pimalaya/base64-codec/blob/master/CHANGELOG.md";
    license = [
      lib.licenses.mit
      lib.licenses.asl20
    ];
    maintainers = with lib.maintainers; [ soywod ];
  };
}
