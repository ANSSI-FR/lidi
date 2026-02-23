# lidi

[![badge_repo](https://img.shields.io/badge/ANSSI--FR-lidi-white)](https://github.com/ANSSI-FR/lidi)
[![badge_catégorie_interne](https://img.shields.io/badge/catégorie-interne-%23d08fce)](https://github.com/ANSSI-FR#types-de-projets)
[![openess_badge_B](https://img.shields.io/badge/code.gouv.fr-open-green)](https://documentation.ouvert.numerique.gouv.fr/les-parcours-de-documentation/ouvrir-un-projet-num%C3%A9rique/#niveau-ouverture)
[![Github CI](https://github.com/ANSSI-FR/lidi/workflows/Rust/badge.svg)](https://github.com/ANSSI-FR/lidi/actions)
[![Github CI](https://github.com/ANSSI-FR/lidi/workflows/Clippy/badge.svg)](https://github.com/ANSSI-FR/lidi/actions)

## French Cybersecurity Agency (ANSSI)

<img src="https://www.sgdsn.gouv.fr/files/styles/ds_image_paragraphe/public/files/Notre_Organisation/logo_anssi.png" alt="ANSSI logo" width="20%">

*This projet is managed by [ANSSI](https://cyber.gouv.fr/). To find out more,
you can go to the
[page](https://cyber.gouv.fr/enjeux-technologiques/open-source/) (in French)
dedicated to the ANSSI open source strategy. You can also click on the badges
above to learn more about their meaning*

## What is lidi?

Lidi (leedee) allows you to copy TCP or Unix streams or files over a unidirectional link.

It is usually used along with an actual network diode device but it can also be used over regular bidirectional links for testing purposes.

For more information about the general purpose and concept of unidirectional networks and data diode: [Unidirectional network](https://en.wikipedia.org/wiki/Unidirectional_network).

## Where to find some documentation?

The *user* documentation is available at <https://anssi-fr.github.io/lidi/>, or can be built and opened with:

```
$ cd doc
$ make html
$ xdg-open _build/html/index.html
```

The *developer* documentation can be built and opened by running:

```
$ cargo doc --document-private-items --no-deps --lib --open
```
