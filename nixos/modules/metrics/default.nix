# This module configures prometheus, grafana and relevant network settings
# for use with the blutgang systemd service nixos module. This does not
# configure blutgang directly, or add any extra functionality so that it
# might serve as a decent example of how to build a configuration ready
# for use *with* the blutgang nixos module.

{ lib, config, ... }:
let
  inherit (lib) optional optionals optionalAttrs;
in
{
  networking.firewall = {
    enable = true;
    allowedTCPPorts = with config.services;
      optionals openssh.enable openssh.ports
      ++ optional grafana.enable grafana.settings.server.http_port
      ++ optionals prometheus.enable [
        prometheus.port
        prometheus.exporters.node.port
      ]
      ++ optional blutgang.enable blutgang.settings.blutgang.port;
  };

  services = {
    openssh.enable = true;

    nginx = {
      enable = true;
      recommendedGzipSettings = true;
      recommendedOptimisation = true;
      recommendedProxySettings = true;
      recommendedTlsSettings = true;
      virtualHosts."${config.services.grafana.settings.server.domain}" = {
        # nginx reverse proxy
        locations."/grafana/" = {
          proxyPass = "http://127.0.0.1:${toString config.services.grafana.settings.server.http_port}";
          proxyWebsockets = true;
          extraConfig = ''
            proxy_set_header Host $host;
          '';
        };
        locations."/prometheus/" = {
          proxyPass = "http://127.0.0.1:${toString config.services.prometheus.port}";
        };
        useACMEHost = config.networking.domain;
      };
    };

    grafana = {
      enable = true;
      dataDir = "/var/lib/grafana";
      settings = {
        "auth.anonymous" = {
          enable = true;
          org_name = "Org";
          org_role = "Editor";
        };
        security = {
          admin_user = "admin";
          admin_password = "admin";
        };
        server = {
          root_url = "%(protocol)s://%(domain)s:%(http_port)s/";
          domain = config.networking.hostName;
          http_port = 2342;
          http_addr = "0.0.0.0";
          protocol = "http";
        };
      };
      provision = {
        enable = true;
        datasources.settings.datasources = [{
          name = "prometheus";
          type = "prometheus";
          url =
            "http://${config.services.prometheus.listenAddress}:9090/prometheus/";
          isDefault = true;
        }];
        dashboards.settings.providers = [
          { name = "Host System Metrics"; folder = "Services"; options.path = ./dashboard.json; }
        ];
      };
    };

    prometheus = {
      enable = true;
      listenAddress = "127.0.0.1";
      webExternalUrl = "https://${config.networking.hostName}/prometheus/";
      port = 9090;
      exporters = {
        node = {
          enable = true;
          enabledCollectors = [ "systemd" ];
          port = 9100;
          openFirewall = true;
          firewallFilter = "-i br0 -p tcp -m tcp --dport 9100";
        };
      };
      scrapeConfigs = [{
        job_name = config.networking.hostName;
        static_configs = [{
          targets = [
            "${config.services.prometheus.listenAddress}:${
              toString config.services.prometheus.exporters.node.port
            }"
          ];
        }];
      }];
    };
  };

  systemd.services.grafana = with config.services; {
    # wait until all network interfaces initilize before starting Grafana
    after = [ "network.target" ] ++ optional blutgang.enable "blutgang.service";
    wants = [ "network.target" ];
  } // optionalAttrs blutgang.enable {
    requires = [ "blutgang.service" ];
  };

  # This value determines the NixOS release from which the default
  # settings for stateful data, like file locations and database versions
  # on your system were taken. It‘s perfectly fine and recommended to leave
  # this value at the release version of the first install of this system.
  # Before changing this value read the documentation for this option
  # (e.g. man configuration.nix or on https://nixos.org/nixos/options.html).
  system.stateVersion = "25.05"; # Did you read the comment?
}
