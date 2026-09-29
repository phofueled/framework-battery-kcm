kcmutils_add_qml_kcm(kcm_framework_battery
    SOURCES
        frameworkbatterykcm.cpp
)

target_link_libraries(kcm_framework_battery PRIVATE
    Qt6::Core
    Qt6::DBus
    Qt6::Gui
    Qt6::Qml
    Qt6::Quick
    KF6::CoreAddons
    KF6::I18n
    KF6::KCMUtilsQuick
)
