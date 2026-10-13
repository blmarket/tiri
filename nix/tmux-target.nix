{ lib, stdenv, tmux, fetchFromGitHub, jemalloc }:

let
  revision = "7f2a35ad3321f9ba57a1062ca73b1f3ff26aca53";
in
tmux.overrideAttrs (old: {
  version = "3.8";
  src = fetchFromGitHub {
    owner = "tmux";
    repo = "tmux";
    rev = revision;
    hash = "sha256-pv2wlr3coo+E0pfpb58giJJQn9PGgMFqtVA6nk+9JdE=";
  };
  patches = [ ];
  buildInputs = (old.buildInputs or [ ])
    ++ lib.optionals stdenv.hostPlatform.isDarwin [ jemalloc ];
  configureFlags =
    (lib.filter (flag: flag != "--enable-sixel") old.configureFlags)
    ++ [ "--disable-sixel" "--disable-debug" ]
    ++ lib.optionals stdenv.hostPlatform.isDarwin [ "--enable-jemalloc" ];
  outputs = (old.outputs or [ "out" ]) ++ [ "source" ];
  postPatch = (old.postPatch or "") + ''
    mkdir -p "$source"
    tar -cf "$source/tmux.tar" .
  '';
  postInstall = (old.postInstall or "") + ''
    mkdir -p "$out/share/tmux"
    printf '%s\n' '${revision}' > "$out/share/tmux/tiri-upstream-revision"
    printf '%s\n' "$source/tmux.tar" > "$out/share/tmux/source"
  '';
})
