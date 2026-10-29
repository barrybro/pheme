// Process setup: what has to happen before the QGuiApplication exists, and after.

#include <QtCore/QUrlQuery>
#include <QtQuickControls2/QQuickStyle>
#include <QtWebEngineCore/QWebEngineUrlRequestInterceptor>
#include <QtWebEngineQuick/QQuickWebEngineProfile>
#include <QtWebEngineQuick/qtwebenginequickglobal.h>

extern "C" void pheme_platform_init()
{
    QtWebEngineQuick::initialize();
    if (qEnvironmentVariableIsEmpty("QT_QUICK_CONTROLS_STYLE")) {
        QQuickStyle::setStyle(QStringLiteral("Fusion"));
    }
}

namespace {

// Frames embedded in an article get the article's origin as their Referer, as they would
// on the article's own site. YouTube's player refuses to play (error 153) without one,
// and pages loaded from files send none. Article pages carry the origin in a `referrer`
// query item (see render.rs).
class EmbedReferrer final : public QWebEngineUrlRequestInterceptor
{
public:
    using QWebEngineUrlRequestInterceptor::QWebEngineUrlRequestInterceptor;

    void interceptRequest(QWebEngineUrlRequestInfo &info) override
    {
        if (info.resourceType() != QWebEngineUrlRequestInfo::ResourceTypeSubFrame)
            return;
        const QUrl page = info.firstPartyUrl();
        if (!page.isLocalFile() || !page.path().contains(QLatin1String("/pheme/article-")))
            return;
        // Frames inside an embed, like YouTube's own, already send a Referer.
        const QString initiator = info.initiator().scheme();
        if (initiator == QLatin1String("http") || initiator == QLatin1String("https"))
            return;
        const QString referrer = QUrlQuery(page).queryItemValue(QStringLiteral("referrer"), QUrl::FullyDecoded);
        const QUrl url(referrer);
        if (url.scheme() != QLatin1String("http") && url.scheme() != QLatin1String("https"))
            return;
        // Chromium cancels requests whose Referer would go from https to http.
        if (url.scheme() == QLatin1String("https") && info.requestUrl().scheme() != QLatin1String("https"))
            return;
        info.setHttpHeader("Referer", url.toEncoded());
    }
};

}

extern "C" void pheme_platform_init_after_app()
{
    auto *profile = QQuickWebEngineProfile::defaultProfile();
    profile->setUrlRequestInterceptor(new EmbedReferrer(profile));
}
