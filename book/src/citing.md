# Citation, sources and license

HejjéTrac was created by **[Pavan Kumar Kaushik](https://github.com/pvnkmrksk)**. It is a modern Rust reworking of FicTrac's spherical-motion tracking, with native desktop, calibration and multi-rig workflows.

The name combines **ಹೆಜ್ಜೆ** (*hejjé*, roughly **HEY-jay**), Kannada for a footstep, footprint or path, with **Trac**, following FicTrac’s spelling: **HEY-jay-track**. [Kannada word reference](https://alar.ink/dictionary/kannada/english/%E0%B2%B9%E0%B3%86%E0%B2%9C%E0%B3%8D%E0%B2%9C%E0%B3%86).

## Cite HejjéTrac

Cite the exact release or commit used in your experiment, along with the original FicTrac paper:

> Pavan Kumar Kaushik. *HejjéTrac: native spherical-motion tracking.* Computer software. https://github.com/pvnkmrksk/hejjetrak. Version or commit used: include your identifier here.

GitHub's **Cite this repository** action uses the repository's [CITATION.cff](https://github.com/pvnkmrksk/hejjetrak/blob/main/CITATION.cff) to export APA or BibTeX. Add the version or commit you actually used. If that release has a Zenodo archive, use its version-specific DOI.

A DOI has not been added to this repository's metadata. The files prepare citation and archiving; they do not register or publish an archive.

## Archive a release on Zenodo

For the repository owner:

1. Sign in to Zenodo and link your GitHub account.
2. Open Zenodo's **GitHub** page, choose **Sync now**, then enable `pvnkmrksk/hejjetrak`.
3. Review [.zenodo.json](https://github.com/pvnkmrksk/hejjetrak/blob/main/.zenodo.json) before the next release. It contains the author, description, license and upstream references. Version and publication date are left to the release import.
4. Publish a versioned GitHub release. Zenodo archives new releases after the repository is connected; check the resulting record and its files.
5. Add the assigned DOI to `CITATION.cff`, this page and the release notes. Keep a version-specific DOI in experiment records; the concept DOI identifies the software across versions.

Zenodo uses `.zenodo.json` in preference to `CITATION.cff` when both exist, so keep their author, title, license and description aligned. The archive JSON uses Zenodo's [license identifier `cc-by-nc-sa-3.0`](https://zenodo.org/api/vocabularies/licenses/cc-by-nc-sa-3.0); the CFF file uses the equivalent SPDX spelling `CC-BY-NC-SA-3.0`. [Enable a repository](https://help.zenodo.org/docs/github/enable-repository/) · [Zenodo metadata guide](https://help.zenodo.org/docs/github/describe-software/zenodo-json/) · [GitHub citation guide](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-citation-files).

## Cite the original method

FicTrac was developed by R. J. D. Moore, Saul Thurrowgood and collaborators. Please also cite:

> RJD Moore, GJ Taylor, AC Paulk, T Pearson, B van Swinderen, MV Srinivasan (2014). *FicTrac: a visual method for tracking spherical motion and generating fictive animal paths.* Journal of Neuroscience Methods 225, 106–119. [doi:10.1016/j.jneumeth.2014.01.010](https://doi.org/10.1016/j.jneumeth.2014.01.010).

Upstream reference: [rjdmoore/fictrac](https://github.com/rjdmoore/fictrac), with this rebuild assessed against the [pvnkmrksk fork](https://github.com/pvnkmrksk/fictrac) at commit `10fc4472af1fc9062b9bf5a92738e835b98cc3ec`.

The original-reference chapters reproduce `doc/params.md`, `doc/coordinate_frames.md`, `doc/requirements.md` and `doc/data_header.txt` at that revision, with attribution and application context. Their original implementation notes describe FicTrac. Coordinate illustrations are retained locally and unchanged; [asset provenance](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/images/upstream/README.md) records their source URLs and hashes. The hardware chapter adapts upstream recommendations and links its additional references. Sample demonstrations and screenshots use the attributed upstream video.

## License

This project and its documentation retain **Creative Commons Attribution–NonCommercial–ShareAlike 3.0** terms. See the [license](https://github.com/pvnkmrksk/hejjetrak/blob/main/LICENSE), [attribution](https://github.com/pvnkmrksk/hejjetrak/blob/main/ATTRIBUTION.md) and [dependency notices](https://github.com/pvnkmrksk/hejjetrak/blob/main/THIRD_PARTY.md). Dependencies retain their own permissive licenses and their notices are included in release archives.

## Design references

[Rekkefly](https://github.com/pvnkmrksk/rekkefly) informed the separate landing page and searchable manual. [PhotoCraft](https://github.com/storytold/photocraft) informed the shared engine boundary. No implementation, imagery, data or fonts were copied from those projects.
