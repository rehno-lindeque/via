{ lib
, stdenv
, makeWrapper
, teetty
, coreutils
, gnugrep
, gnused
, findutils
}:

stdenv.mkDerivation rec {
  pname = "via";
  version = "0.1.0";

  src = ./.;

  nativeBuildInputs = [ makeWrapper ];

  buildInputs = [ teetty ];

  dontBuild = true;

  installPhase = ''
    runHook preInstall

    mkdir -p $out/bin
    cp via $out/bin/via
    chmod +x $out/bin/via

    # Wrap the script to ensure all dependencies are in PATH
    wrapProgram $out/bin/via \
      --prefix PATH : ${lib.makeBinPath [
        teetty
        coreutils  # provides tail, tac, mkdir, etc.
        gnugrep    # provides grep
        gnused     # provides sed
        findutils  # provides find
      ]}

    runHook postInstall
  '';

  meta = with lib; {
    description = "Hacky script for use with teetty";
    homepage = "https://github.com/yourusername/via";
    license = licenses.mit;
    maintainers = [ ];
    platforms = platforms.unix;
    mainProgram = "via";
  };
}
