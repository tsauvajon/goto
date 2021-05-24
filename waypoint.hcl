project = "track-my-macros"

  app "track-my-macros" {

    build {
      use "pack" {}
      registry {
          use "docker" {
            image = "track-my-macros"
            tag = "1"
            local = true
          }
      }
  }

    deploy {
      use "nomad" {
        datacenter = "dc1"
      }
    }
  }
