cmake_minimum_required(VERSION 3.20)
project(framework_battery VERSION 0.1.0 LANGUAGES CXX)

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
