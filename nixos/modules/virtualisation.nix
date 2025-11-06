# WARNING: For testing purposes only.
#
# Enables port forwarding for SSH and Grafana from a VM to the host machine.
# This does make assumptions about the host machine's available ports, so it
# is possible for this to fail due to the host port not being available.

{ lib, config, modulesPath, ... }:
{
  imports = [
    (modulesPath + "/virtualisation/qemu-vm.nix")
  ];

  virtualisation = {
    cores = 3;
    forwardPorts = with config.services;
      lib.optional openssh.enable
        {
          from = "host";
          host.port = 2222;
          guest.port = 22;
        }
      ++ lib.optional grafana.enable
        {
          from = "host";
          host.port = grafana.settings.server.http_port;
          guest.port = grafana.settings.server.http_port;
        };
    diskSize = 25000;
    memorySize = 4096;
  };
}
