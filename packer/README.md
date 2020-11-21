Create a base image for our test (login: tansiv/ psswd: tansiv)

```
packer build -only qemu debian-10.3.0-x86_64.json
```

https://learn.hashicorp.com/tutorials/packer/getting-started-install