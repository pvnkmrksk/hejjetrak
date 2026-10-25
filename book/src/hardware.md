# Experimental setup

A good image makes calibration easier and tracking more reliable. Choose the ball pattern, lens and lighting together, then fix the camera settings before saving a calibration.

This guide adapts the [FicTrac hardware recommendations](https://github.com/pvnkmrksk/fictrac/blob/10fc4472af1fc9062b9bf5a92738e835b98cc3ec/doc/requirements.md), by R. J. D. Moore and collaborators, under CC BY-NC-SA 3.0. The [complete original setup and algorithm guide](upstream-setup.md) is also preserved in this manual. Additional lighting and field-of-view notes are identified below.

## Pattern the ball

Use a lightweight sphere with a matte, high-contrast, irregular pattern. Aim for roughly equal light and dark coverage across the surface. Upstream recommends around **10–50 varied blobs**, mixing larger landmarks with smaller features. Use that as a starting point for the size of your ball and its image.

- Distribute rounded, uneven blobs over the whole sphere. Vary their size, contour and spacing.
- Mix coarse and medium detail, with a few finer features that remain visible in the thresholded ROI.
- Avoid regular football panels, repeated stripes, grids and evenly spaced dots: similar views can become ambiguous.
- Avoid relying on tiny speckles, thin spikes or hairline gaps. Resampling and filtering can erase them or make them flicker between frames.
- Check dried ink for glare. A bright reflection stays tied to the light source rather than moving with the painted surface.

Look at the **thresholded ROI**, not just the full-resolution camera image. The pattern must remain recognizable at the tracking resolution chosen by `q_factor`. Increasing camera pixels alone cannot restore details removed by the tracking pipeline.

Keep as much patterned surface visible as practical. Mask the animal, tether and stationary supports, while leaving enough sphere to identify a pose. Upstream's starting recommendation is at least half of one hemisphere unobscured; better visibility generally gives the fitter more information.

![Upstream example of an irregular ball pattern and calibrated lab axes](images/upstream/lab-axes.png)

*FicTrac configuration example, R. J. D. Moore and contributors, CC BY-NC-SA 3.0. [Source and provenance](https://github.com/pvnkmrksk/hejjetrak/blob/main/docs/images/upstream/README.md). This is an upstream illustration.*

## Give tracking its own light

A dedicated diffuse light source keeps the ball visible when visual stimuli change. Place and diffuse it to avoid glare and deep shadows at the sphere's edges. A near-infrared illuminator, such as a suitable CCTV ring light, can be useful when tracking illumination must be separated from visible stimuli.

For IR, check the complete optical path. The camera must be sensitive at the illuminator's wavelength; an **IR-cut** filter blocks that light. A matched **IR-pass** filter can reduce visible stimulus leakage into the camera. A filter that merely looks black is not a specification: verify its transmission range, then check the actual image under every stimulus condition. See [Axis's explanation of IR illumination and filters](https://whitepapers.axis.com/en-us/ir-in-surveillance).

Set exposure short enough that the pattern stays sharp at the fastest expected rotation. Add light before compensating with excessive gain or a long exposure. Where supported, fix exposure, gain, white balance and focus once they are correct; automatic changes can alter the thresholded pattern during an experiment. Recheck focus under the final illumination.

### Red and black patterns

A thin red base coat with dark markings is a possible lab technique for reducing visible pattern contrast while keeping contrast in an IR camera. **Test the actual inks**: some red inks reflect near-IR strongly and some black inks do too. Camera contrast depends on the ink, substrate, wavelength and filter combination, rather than the color name on the marker.

Do not assume that red or infrared illumination is invisible to every insect. Species, genotype, wavelength and intensity matter; even Drosophila can respond to long-wavelength light under some conditions. Choose and validate illumination for the animal and assay. See the primary studies on [Drosophila spectral sensitivity](https://pmc.ncbi.nlm.nih.gov/articles/PMC7588446/) and [red-light responses](https://pmc.ncbi.nlm.nih.gov/articles/PMC10235467/).

## Measure field of view before mounting

`vfov` is the **vertical angular field of the whole delivered image**, in degrees. It is neither the ball's angular diameter nor a value derivable from image width and height alone. Camera specifications often quote diagonal or horizontal field of view; those are different quantities.

For an ordinary rectilinear lens, a practical bench estimate is:

1. Select the final camera resolution, lens focus and zoom. Disable stabilization or digital cropping where possible.
2. Put a flat ruler or target perpendicular to the optical axis, centered in the image.
3. Measure the physical height **H** spanning the full image, from its top edge to its bottom edge at the target plane.
4. Measure distance **D** along the optical axis from the lens's effective viewpoint to the target. Use the same units for H and D.
5. Calculate `vfov = 2 × atan(H / (2 × D)) × 180 / π`.

![Rectilinear vertical field-of-view measurement](images/vfov-measurement.svg)

For example, a visible height of 200 mm at a distance of 300 mm gives approximately **36.87°**. Use a sufficiently distant target so uncertainty in the lens viewpoint is small relative to D. Repeat the measurement to check it. Calibrated camera intrinsics or a verified specification for the exact sensor mode are preferable when available.

This triangle formula assumes a centered rectilinear image. It is not a fisheye calibration procedure. HejjéTrac's `fisheye` option uses an equidistant projection; select it only when that model matches the lens. Cropping, aspect-ratio changes, sensor modes and digital zoom can change effective FOV. Recheck after such changes, even if the lens stays mounted.

A wrong `vfov` can still produce a plausible outline. Check the lab axes and known physical ball motion after completing [calibration](calibration.md). The [upstream frame diagram](upstream-coordinates.md) explains camera, animal and world orientation.

## Choose resolution and frame rate

Choose the lowest native resolution that preserves a sharp, distinctive pattern. A high-resolution stream increases capture, conversion and recording work even when the tracking ROI is small. Choose frame rate and exposure for the fastest expected movement, rather than a stationary ball.

A 100, 120 or 240 Hz source has a frame interval of 10, 8.33 or 4.17 ms. That budget includes acquisition, conversion, tracking and outputs. Watch delivered FPS, processing time and capture skips during a full-length test with your intended recording settings.

Each session tracks on CPU; the GPU draws the native preview. More cores help independent rigs, but USB bandwidth, drivers, storage and encoding still matter. Spread cameras across suitable host controllers where needed and test all sources together. [Several rigs](rigs.md) covers grid launch and distinct output ports.

## Save a reproducible setup

Save the config with the camera mode, lens/FOV, calibration and outputs for that rig. Keep a short reference recording, a photograph of the mounting and the ball/ink/illumination details with the experiment. After any hardware change, review the outline, exclusions and axes before collecting data.

Camera support and execution evidence are listed in [camera controls](camera-controls.md), [compatibility](parity.md) and [validation](validation.md). Industrial SDK-only cameras and vendor-specific controls are outside the current native capture adapters.
