vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO lu-zero/mfx_dispatch
    REF "${VERSION}"
    SHA512 12517338342d3e653043a57e290eb9cffd190aede0c3a3948956f1c7f12f0ea859361cf3e534ab066b96b1c211f68409c67ef21fd6d76b68cc31daef541941b0
    HEAD_REF master
    PATCHES
        fix-unresolved-symbol.patch
        fix-pkgconf.patch
        0003-upgrade-cmake-3.14.patch
)

file(READ "${SOURCE_PATH}/src/mfxparser.cpp" MFXPARSER_CPP)
if(NOT MFXPARSER_CPP MATCHES "#include <cstdint>")
    set(MFXPARSER_CPP_ORIG "${MFXPARSER_CPP}")
    string(REPLACE
        "#include <sstream>\n\n#include \"mfxloader.h\""
        "#include <sstream>\n#include <cstdint>\n\n#include \"mfxloader.h\""
        MFXPARSER_CPP
        "${MFXPARSER_CPP}"
    )
    if(MFXPARSER_CPP STREQUAL MFXPARSER_CPP_ORIG)
        message(FATAL_ERROR "Failed to inject <cstdint> include into src/mfxparser.cpp")
    endif()
    file(WRITE "${SOURCE_PATH}/src/mfxparser.cpp" "${MFXPARSER_CPP}")
endif()

if(VCPKG_TARGET_IS_WINDOWS AND NOT VCPKG_TARGET_IS_MINGW)
    vcpkg_cmake_configure(
        SOURCE_PATH "${SOURCE_PATH}" 
    )
    vcpkg_cmake_install()
    vcpkg_copy_pdbs()
else()
    if(VCPKG_TARGET_IS_MINGW)
        vcpkg_check_linkage(ONLY_STATIC_LIBRARY)
    endif()
    vcpkg_configure_make(
        SOURCE_PATH "${SOURCE_PATH}"
        AUTOCONFIG
    )
    vcpkg_install_make()
endif()
vcpkg_fixup_pkgconfig()
  
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/include")
vcpkg_install_copyright(FILE_LIST "${SOURCE_PATH}/LICENSE")
