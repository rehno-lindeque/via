{ lib
, rustPlatform
, makeWrapper
, teetty
, coreutils
}:

rustPlatform.buildRustPackage rec {
  pname = "via";
  version = "0.2.0";

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  nativeBuildInputs = [ makeWrapper ];

  # Wrap the binary to ensure runtime dependencies are in PATH
  postInstall = ''
    wrapProgram $out/bin/via \
      --prefix PATH : ${lib.makeBinPath [
        teetty      # Required for 'via run' command
        coreutils   # Required for 'tail' in tail.rs
      ]}
  '';

  meta = with lib; {
    description = "Issue commands across multiple interactive CLI sessions";
    homepage = "https://github.com/rehno-lindeqe/via";
    license = licenses.asl20;
    maintainers = [ ];
    platforms = platforms.unix;
    mainProgram = "via";
  };
}
