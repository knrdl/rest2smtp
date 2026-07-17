{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.rest2smtp;

  encryptionType = lib.types.enum [
    "TLS"
    "STARTTLS"
    "UNENCRYPTED"
  ];

  defaultPackage = pkgs.callPackage ./package.nix { };

  runtimeDir = "/run/rest2smtp";

  startScript = pkgs.writeShellScript "rest2smtp-start" ''
    set -euo pipefail
    runtimeDir=''${RUNTIME_DIRECTORY:-${runtimeDir}}
    rm -rf "$runtimeDir"/*
    cp -a ${cfg.package}/share/rest2smtp/. "$runtimeDir/"
    # Nix store files are read-only; rest2smtp rewrites openapi.yaml at startup.
    chmod -R u+w "$runtimeDir"
    ${lib.optionalString (cfg.smtp.passwordFile != null) ''
      export SMTP_PASSWORD=$(tr -d '\n\r' < "$CREDENTIALS_DIRECTORY/smtp.pass")
    ''}
    ${lib.optionalString (cfg.apiTokenFile != null) ''
      export API_TOKEN=$(tr -d '\n\r' < "$CREDENTIALS_DIRECTORY/api.token")
    ''}
    cd "$runtimeDir"
    exec ${lib.getExe cfg.package}
  '';
in
{
  options.services.rest2smtp = {
    enable = lib.mkEnableOption "rest2smtp REST-to-SMTP mail gateway";

    package = lib.mkOption {
      type = lib.types.package;
      default = defaultPackage;
      description = "rest2smtp package to run.";
    };

    listenAddress = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address the HTTP API listens on.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 80;
      description = "Port the HTTP API listens on.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open the configured TCP port in the host firewall.";
    };

    apiDocInfo = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "Internal mail relay for example.org";
      description = ''
        Custom text shown in the API documentation header.
        When unset, the upstream default is used.
      '';
    };

    apiToken = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = ''
        Shared bearer token for `POST /send`.
        Prefer {option}`services.rest2smtp.apiTokenFile` for secrets.
        When unset (and no token file), the API stays open.
      '';
    };

    apiTokenFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      example = "/etc/rest2smtp.token";
      description = ''
        File containing the shared bearer token for `POST /send`.
        Prefer deploying this with NixOps `deployment.keys` to the target path.
        When unset (and no inline token), the API stays open.
      '';
    };

    environmentFile = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      example = "/etc/rest2smtp.env";
      description = ''
        Optional environment file with additional variables for the service.
        Values from the module options take precedence over this file.
        Prefer deploying secrets with NixOps `deployment.keys` to the target path.
      '';
    };

    smtp = {
      host = lib.mkOption {
        type = lib.types.str;
        default = "";
        example = "smtp.example.org";
        description = "SMTP server hostname.";
      };

      port = lib.mkOption {
        type = lib.types.nullOr lib.types.port;
        default = null;
        example = 587;
        description = "SMTP server port. When unset, rest2smtp picks a default based on encryption.";
      };

      encryption = lib.mkOption {
        type = encryptionType;
        default = "TLS";
        description = "SMTP transport encryption method.";
      };

      username = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "SMTP authentication username.";
      };

      password = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = ''
          SMTP authentication password.
          Prefer {option}`services.rest2smtp.smtp.passwordFile` for secrets.
        '';
      };

      passwordFile = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        example = "/etc/rest2smtp.pass";
        description = ''
          File containing the SMTP authentication password.
          Prefer deploying this with NixOps `deployment.keys` to the target path.
        '';
      };
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.smtp.host != "";
        message = "services.rest2smtp.smtp.host must be set when rest2smtp is enabled.";
      }
      {
        assertion = !(cfg.smtp.password != null && cfg.smtp.passwordFile != null);
        message = "Set only one of services.rest2smtp.smtp.password or services.rest2smtp.smtp.passwordFile.";
      }
      {
        assertion = !(cfg.apiToken != null && cfg.apiTokenFile != null);
        message = "Set only one of services.rest2smtp.apiToken or services.rest2smtp.apiTokenFile.";
      }
    ];

    systemd.services.rest2smtp = {
      description = "rest2smtp REST-to-SMTP mail gateway";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];

      path = with pkgs; [
        coreutils
      ];

      environment = lib.filterAttrs (_: v: v != null) {
        ROCKET_ADDRESS = cfg.listenAddress;
        ROCKET_PORT = toString cfg.port;
        ROCKET_PROFILE = "release";
        SMTP_HOST = cfg.smtp.host;
        SMTP_PORT = if cfg.smtp.port != null then toString cfg.smtp.port else null;
        SMTP_ENCRYPTION = cfg.smtp.encryption;
        SMTP_USERNAME = cfg.smtp.username;
        SMTP_PASSWORD = cfg.smtp.password;
        API_TOKEN = cfg.apiToken;
        API_DOC_INFO = cfg.apiDocInfo;
      };

      serviceConfig = {
        Type = "simple";
        DynamicUser = true;
        RuntimeDirectory = "rest2smtp";
        WorkingDirectory = runtimeDir;
        ExecStart = startScript;
        Restart = "on-failure";
        RestartSec = "5s";

        LoadCredential =
          lib.optional (cfg.smtp.passwordFile != null) "smtp.pass:${toString cfg.smtp.passwordFile}"
          ++ lib.optional (cfg.apiTokenFile != null) "api.token:${toString cfg.apiTokenFile}";

        AmbientCapabilities = lib.mkIf (cfg.port < 1024) [ "CAP_NET_BIND_SERVICE" ];
        CapabilityBoundingSet = lib.mkIf (cfg.port < 1024) [ "CAP_NET_BIND_SERVICE" ];

        NoNewPrivileges = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectControlGroups = true;
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" ];
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        RemoveIPC = true;
        SystemCallArchitectures = "native";
      } // lib.optionalAttrs (cfg.environmentFile != null) {
        EnvironmentFile = cfg.environmentFile;
      };

    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [ cfg.port ];
  };
}
