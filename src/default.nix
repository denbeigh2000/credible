{ }:

let
  sampleSecret = {
    name = "sampleKey";
    path = "/nix/store/nix/...";
    encryptionKeys = [
      # List of public key contents
    ];
  };

  sampleRuntimeSecretMount = {
    privateKeyPath = "/home/denbeigh/.ssh/id_rsa";
    secret = sampleSecret;
  };

  sampleRuntimeSecretMountRecord = {
    path = "/var/run/.../1/secretName";
    mount = sampleRuntimeSecretMount;
  };
in

{
  secrets = [ sampleSecret ];
}
