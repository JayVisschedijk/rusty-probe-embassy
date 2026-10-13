{ pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/archive/refs/tags/26.05.tar.gz") {} }:

pkgs.mkShell {
  buildInputs = [
    pkgs.probe-rs-tools
  ];

  shellHook = ''
    echo "Shell ready!"
  '';
}
