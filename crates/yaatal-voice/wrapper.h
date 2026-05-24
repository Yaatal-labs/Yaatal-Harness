/* Thin wrapper header for bindgen.
 *
 * bindgen is invoked against this file in build.rs.  We use a bare filename
 * (no path) because build.rs passes `-I<speech-core-include-dir>` to clang,
 * which makes the include path available for resolution.
 */
#include "speech_core/speech_core_c.h"
