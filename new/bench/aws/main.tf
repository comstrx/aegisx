terraform {
  required_version = ">= 1.6"
  required_providers {
    aws = { source = "hashicorp/aws", version = "~> 6.0" }
  }
}

provider "aws" {
  region = var.region
  default_tags {
    tags = { project = "aegisx-bench", owner = "comstrx" }
  }
}

variable "region" {
  type    = string
  default = "eu-north-1"
}

variable "zone" {
  type    = string
  default = "eu-north-1a"
}

variable "public_key" {
  type = string
}

variable "admin_cidrs" {
  type = list(string)
}

variable "proxy_type" {
  type    = string
  default = "c7i-flex.large"
}

variable "load_type" {
  type    = string
  default = "c7i-flex.large"
}

variable "backend_type" {
  type    = string
  default = "c7i-flex.large"
}

variable "build_type" {
  type    = string
  default = "m7i-flex.large"
}

variable "bench" {
  type    = bool
  default = false
}

variable "disk_gb" {
  type    = number
  default = 40
}

data "aws_ssm_parameter" "ubuntu" {
  name = "/aws/service/canonical/ubuntu/server/24.04/stable/current/amd64/hvm/ebs-gp3/ami-id"
}

resource "aws_vpc" "bench" {
  cidr_block           = "10.42.0.0/16"
  enable_dns_hostnames = true
  tags                 = { Name = "aegisx-bench" }
}

resource "aws_subnet" "bench" {
  vpc_id                  = aws_vpc.bench.id
  cidr_block              = "10.42.1.0/24"
  availability_zone       = var.zone
  map_public_ip_on_launch = true
  tags                    = { Name = "aegisx-bench" }
}

resource "aws_internet_gateway" "bench" {
  vpc_id = aws_vpc.bench.id
  tags   = { Name = "aegisx-bench" }
}

resource "aws_route_table" "bench" {
  vpc_id = aws_vpc.bench.id
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = aws_internet_gateway.bench.id
  }
  tags = { Name = "aegisx-bench" }
}

resource "aws_route_table_association" "bench" {
  subnet_id      = aws_subnet.bench.id
  route_table_id = aws_route_table.bench.id
}

resource "aws_security_group" "bench" {
  name        = "aegisx-bench"
  description = "aegisx benchmark nodes"
  vpc_id      = aws_vpc.bench.id

  ingress {
    description = "ssh from admins"
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = var.admin_cidrs
  }

  ingress {
    description = "everything inside the group"
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    self        = true
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = { Name = "aegisx-bench" }
}

resource "aws_key_pair" "bench" {
  key_name   = "aegisx-bench"
  public_key = var.public_key
}

locals {
  bench = {
    proxy   = var.proxy_type
    load    = var.load_type
    backend = var.backend_type
  }
  nodes = merge({ build = var.build_type }, { for name, type in local.bench : name => type if var.bench })
}

resource "aws_instance" "node" {
  for_each = local.nodes

  ami                         = data.aws_ssm_parameter.ubuntu.value
  instance_type               = each.value
  subnet_id                   = aws_subnet.bench.id
  vpc_security_group_ids      = [aws_security_group.bench.id]
  key_name                    = aws_key_pair.bench.key_name
  associate_public_ip_address = true
  user_data                   = file("${path.module}/bootstrap.sh")

  root_block_device {
    volume_type = "gp3"
    volume_size = var.disk_gb
  }

  lifecycle {
    ignore_changes = [ami, user_data]
  }

  tags = { Name = "aegisx-bench-${each.key}", role = each.key }
}

output "public_ips" {
  value = { for name, node in aws_instance.node : name => node.public_ip }
}

output "private_ips" {
  value = { for name, node in aws_instance.node : name => node.private_ip }
}
