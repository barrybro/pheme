// Process setup that has to happen before the QGuiApplication exists.

#include <QtQuickControls2/QQuickStyle>
#include <QtWebEngineQuick/qtwebenginequickglobal.h>

extern "C" void pheme_platform_init()
{
    QtWebEngineQuick::initialize();
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("Fusion"));
    }
}
