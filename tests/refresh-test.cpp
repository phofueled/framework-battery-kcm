#include <KPluginMetaData>
#include <KQuickConfigModule>
#include <KQuickConfigModuleLoader>
#include <QApplication>
#include <QDialog>
#include <QElapsedTimer>
#include <QQmlComponent>
#include <QQmlContext>
#include <QQmlEngine>
#include <QQuickItem>
#include <QQuickWidget>
#include <QQuickWindow>
#include <QThread>
#include <QVBoxLayout>

#include <iostream>
#include <memory>

int main(int argc, char **argv) {
    QApplication app(argc, argv);
    if (argc != 2) {
        return 1;
    }
    const auto result = KQuickConfigModuleLoader::loadModule(KPluginMetaData(QString::fromLocal8Bit(argv[1])));
    const std::unique_ptr<KQuickConfigModule> moduleOwner(result.plugin);
    auto *module = moduleOwner.get();
    if (!module || !module->mainUi()) {
        std::cerr << "Could not load the battery pane\n";
        return 1;
    }
    auto *page = module->mainUi();
    module->engine()->rootContext()->setContextProperty(QStringLiteral("batteryPage"), page);
    QQmlComponent host(module->engine().get());
    host.setData(R"(
        import QtQuick
        import org.kde.kirigami as Kirigami
        Kirigami.PageRow {
            id: pages
            Component.onCompleted: push(batteryPage)
            function openOtherPage() { push(otherPage) }
            function returnToBattery() { pop() }
            Component { id: otherPage; Kirigami.Page {} }
        }
    )", QUrl());
    auto *container = host.create();
    if (!container) {
        std::cerr << "Could not create the KCM page host\n";
        return 1;
    }
    QDialog window;
    QQuickWidget widget(module->engine().get(), &window);
    QVBoxLayout layout(&window);
    layout.addWidget(&widget);
    widget.setResizeMode(QQuickWidget::SizeRootObjectToView);
    window.resize(680, 480);
    widget.setContent(QUrl(), &host, container);

    const auto check = [&app, module](bool expected, const char *message) {
        QElapsedTimer elapsed;
        elapsed.start();
        do {
            app.processEvents();
            if (module->property("refreshEnabled").toBool() == expected) {
                return true;
            }
            QThread::msleep(1);
        } while (elapsed.elapsed() < 1000);
        std::cerr << message << '\n';
        return false;
    };
    bool passed = check(false, "A pane in a hidden window must not poll");
    window.show();
    passed &= check(true, "Showing the pane must enable refreshes");
    window.hide();
    passed &= check(false, "Hiding the window must stop refreshes");
    window.show();
    passed &= check(true, "Showing the window again must resume refreshes");
    // Wayland can suspend the native host without changing QQuickWidget's
    // synthetic window visibility, as happens on external KWin minimization.
    window.windowHandle()->hide();
    widget.quickWindow()->setVisible(true);
    passed &= check(false, "A hidden native host must stop refreshes");
    if (widget.quickWindow()->visibility() != QWindow::Windowed) {
        std::cerr << "The native-host test must leave the synthetic window visible\n";
        passed = false;
    }
    window.windowHandle()->show();
    passed &= check(true, "Restoring the native host must resume refreshes");
    page->setVisible(false);
    passed &= check(false, "Hiding the battery page must stop refreshes");
    page->setVisible(true);
    passed &= check(true, "Returning to the battery page must resume refreshes");
    QMetaObject::invokeMethod(container, "openOtherPage");
    passed &= check(false, "Opening another page must stop battery refreshes");
    QMetaObject::invokeMethod(container, "returnToBattery");
    passed &= check(true, "Returning from another page must resume battery refreshes");
    window.showMinimized();
    passed &= check(false, "A minimized window must not poll");
    window.showNormal();
    passed &= check(true, "Restoring the window must resume refreshes");
    window.hide();
    return passed ? 0 : 1;
}
