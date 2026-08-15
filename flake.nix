{
  description = "minimalbase + radarr-auto-add-people service";
  inputs = {
    nixpkgs.follows = "minimalbase/nixpkgs";
    minimalbase.url = "github:nonrootdocker/minimalbase";
  };
  outputs = { self, nixpkgs, minimalbase }:
  let
    system = "x86_64-linux";
    pkgs = import nixpkgs {
      inherit system;
      config.allowUnfree = true;
    };

    cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);

    # ----------------------------
    # radarr-auto-add-people package (built from source in this repo).
    # TLS is rustls, so there is no openssl dependency to drag in.
    # ----------------------------
    radarr-auto-add-people = pkgs.rustPlatform.buildRustPackage {
      pname = cargoToml.package.name;
      version = cargoToml.package.version;
      src = ./.;
      cargoLock.lockFile = ./Cargo.lock;
    };

    # ----------------------------
    # Crate version, exposed as the `version` output for CI tagging.
    # ----------------------------
    version = pkgs.writeText "radarr-auto-add-people-version" cargoToml.package.version;

    # ----------------------------
    # User database configuration (/etc/passwd)
    # ----------------------------
    passwdFile = pkgs.writeTextDir "etc/passwd" ''
      root:x:0:0:root:/root:/bin/sh
      radarr-auto-add:x:1000:1000:radarr-auto-add:/data:/bin/sh
    '';

    # ----------------------------
    # ABI descriptor for container-init
    # ----------------------------
    abi = pkgs.writeTextFile {
      name = "radarr-auto-add-people-abi.json";
      text = builtins.toJSON {
        version = 2;
        process = {
          exec = "${radarr-auto-add-people}/bin/radarr-auto-add-people";
          # One-shot job: scan TMDB, add to Radarr, exit.
          mode = "triggered";
          args = [ ];
          cwd = "/data";
          dirs = [ "/data" ];
        };
      };
      destination = "/app/main";
    };

  in {
    packages.${system} = {
      default = self.packages.${system}.radarr-auto-add-people-image;
      inherit radarr-auto-add-people version;

      radarr-auto-add-people-image = pkgs.dockerTools.buildImage {
        name = "radarr-auto-add-people";
        tag = "latest";
        fromImage = minimalbase.packages.${system}.base-image;
        copyToRoot = pkgs.buildEnv {
          name = "root";
          paths = [
            pkgs.coreutils
            pkgs.tzdata
            pkgs.cacert
            radarr-auto-add-people
            abi
            passwdFile
          ];
        };
        config = {
          Entrypoint = [ "${minimalbase.packages.${system}.container-init}/bin/container-init" ];
          User = "1000:1000";
          Env = [
            "PATH=/bin"
            "TZ=UTC"
            "LANG=en_US.UTF-8"
            "CONFIG_FILE=/config/config.toml"
            "RUST_LOG=info"
          ];
        };
      };
    };

    checks.${system}.build = self.packages.${system}.radarr-auto-add-people;

    devShells.${system}.default = pkgs.mkShell {
      packages = [ pkgs.cargo pkgs.rustc pkgs.rustfmt pkgs.clippy ];
    };
  };
}
