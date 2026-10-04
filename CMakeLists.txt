cmake_minimum_required(VERSION 3.20)
project(framework_battery VERSION 0.3.1 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(ECM 6.0 REQUIRED NO_MODULE)
set(CMAKE_MODULE_PATH ${ECM_MODULE_PATH})
include(KDEInstallDirs)
include(KDECMakeSettings)
include(KDECompilerSettings NO_POLICY_SCOPE)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core DBus Gui Qml Quick)
find_package(KF6 6.0 REQUIRED COMPONENTS CoreAddons I18n KCMUtils)

add_subdirectory(kcm)

include(CTest)
if(BUILD_TESTING)
    find_package(Qt6 6.5 REQUIRED COMPONENTS Widgets QuickWidgets)
    add_executable(batterycapacity-test tests/batterycapacity-test.cpp kcm/batterycapacity.cpp)
    target_include_directories(batterycapacity-test PRIVATE kcm)
    target_link_libraries(batterycapacity-test PRIVATE Qt6::Core)
    add_test(NAME batterycapacity COMMAND batterycapacity-test)
    add_executable(refresh-test tests/refresh-test.cpp)
    target_link_libraries(refresh-test PRIVATE Qt6::Widgets Qt6::QuickWidgets KF6::KCMUtilsQuick)
    add_dependencies(refresh-test kcm_framework_battery)
    add_test(NAME refresh-visibility COMMAND refresh-test $<TARGET_FILE:kcm_framework_battery>)
    set_tests_properties(refresh-visibility PROPERTIES
        ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_QUICK_BACKEND=software")
endif()
