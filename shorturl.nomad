job "shorturl" {
  datacenters = ["dc1"]

  type = "service"

  update {
    max_parallel = 1
    min_healthy_time = "5s"
    healthy_deadline = "1m"
    progress_deadline = "10m"
    auto_revert = false
    canary = 0
  }

  migrate {
    max_parallel = 1
    health_check = "checks"
    min_healthy_time = "10s"
    healthy_deadline = "5m"
  }

  group "shorturl" {
    count = 1

    network {
      port "http" {
        to = 8080
      }
    }

    # The "service" stanza instructs Nomad to register this task as a service
    # in the service discovery engine, which is currently Consul. This will
    # make the service addressable after Nomad has placed it on a host and
    # port.
    #
    # For more information and examples on the "service" stanza, please see
    # the online documentation at:
    #
    #     https://www.nomadproject.io/docs/job-specification/service
    #
    service {
      name = "shorturl-api"
      tags = ["global"]
      port = "http"

      # The "check" stanza instructs Nomad to create a Consul health check for
      # this service. A sample check is provided here for your convenience;
      # uncomment it to enable it. The "check" stanza is documented in the
      # "service" stanza documentation.

      # check {
      #   name     = "alive"
      #   type     = "tcp"
      #   interval = "10s"
      #   timeout  = "2s"
      # }
    }

    restart {
      attempts = 2
      interval = "30m"
      delay = "15s"
      mode = "fail"
    }

    ephemeral_disk {
      sticky = true
      migrate = true
      size = 10 # 10 Mb
    }

    task "api" {
      driver = "raw_exec"
      
      config {
        command = "./shorturl"
        args = []
      }

      // volume_mount {
      //   volume      = "dist"
      //   destination = "/front/dist"
      //   read_only   = false
      // }

      // artifact {
      //   source = "/target/release/shorturl"
      // }

      logs {
        max_files     = 3
        max_file_size = 1
      }

      resources {
        cpu    = 500 # 500 MHz
        memory = 256 # 256MB
      }
    }
  }
}
