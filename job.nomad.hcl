job "hive" {
  namespace = "auth"

  type = "service"

  group "hive" {
    network {
      port "http" { }
    }

    service {
      name     = "hive"
      port     = "http"
      provider = "nomad"
      tags = [
        "traefik.enable=true",
        "traefik.http.routers.hive.rule=Host(`hive.datasektionen.se`)",
        "traefik.http.routers.hive.tls.certresolver=default",

        "traefik.http.routers.hive-internal.rule=Host(`hive.nomad.dsekt.internal`)",
        "traefik.http.routers.hive-internal.entrypoints=web-internal",
      ]
    }

    task "hive" {
      driver = "docker"

      config {
        image = var.image_tag
        ports = ["http"]
      }

      template {
        data        = <<ENV
HIVE_PORT={{ env "NOMAD_PORT_http" }}
{{ with nomadVar "nomad/jobs/hive" }}
HIVE_DB_URL=postgres://hive:{{ .db_password }}@postgres.dsekt.internal:5432/hive
{{ end }}
TZ=Europe/Stockholm
ENV
        destination = "local/.env"
        env         = true
      }
    }
  }
}

variable "image_tag" {
  type = string
  default = "ghcr.io/datasektionen/hive:latest"
}
