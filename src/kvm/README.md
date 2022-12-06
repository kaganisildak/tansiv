# Tansiv-timer
Il s'agit d'un module noyau (expérimental) pour faire fonctionner tansiv avec de
la virtualisation KVM, grâce à KVM. Pour cela sont utilisé les
[hrtimers](https://elixir.bootlin.com/linux/latest/source/include/linux/hrtimer.h),
timers hautes résolution utilisables depuis un module noyau.

## Compilation et installation
### Kernelspace
```bash
# Dépendances
sudo apt install build-essential kmod linux-headers-`uname -r`
# Compilation
cd Kernelspace/
make
# Chargement du module
sudo insmod tansiv-timer.ko
# Déchargement du module
sudo rmmod tansiv-timer
```
* Sur certaines distributions (Fedora) les modules doivent être signés avant le chargement
  avec insmod
* Entre 2 utilisations, décharger puis recharger le module pour que tout soit
  bien réinitialisé

### Userspace
* API pour communiquer avec le module noyau grâce à des ioctls
* A ajuster (en particulier le header) pour s'intégrer dans l'application userspace