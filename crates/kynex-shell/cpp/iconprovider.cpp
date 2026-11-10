#include "iconprovider.h"
#include <QtSvg/QSvgRenderer>
#include <QPainter>
#include <QFile>
#include <QFileInfo>
#include <QUrlQuery>
#include <QUrl>
#include <QXmlStreamReader>
#include <QtQml/QQmlEngine>
#include <QtQml/QQmlApplicationEngine>
#include <cstring>

extern "C" bool kynex_resolve_icon(const char *name, uint32_t size, uint32_t scale, char *out_buf, size_t buf_size);

void kynex_init_security_limits()
{
    // Hard clamp allocation limit for raster images to 16 MB
    QImageReader::setAllocationLimit(16);
}

bool kynex_is_svg_safe(const QByteArray &data)
{
    QByteArray lower = data.toLower();

    // 1. Rejection of external DTDs, external entities, CSS @import, and raw file:// or ftp:// URIs
    if (lower.contains("<!doctype") || lower.contains("<!entity") ||
        lower.contains("@import") || lower.contains("file://") ||
        lower.contains("ftp://")) {
        return false;
    }

    // 2. Parse XML stream looking for forbidden external references or <use> bombs
    QXmlStreamReader xml(data);
    int useCount = 0;

    while (!xml.atEnd() && !xml.hasError()) {
        QXmlStreamReader::TokenType token = xml.readNext();
        if (token == QXmlStreamReader::StartElement) {
            QStringView name = xml.name();
            if (name == u"use") {
                useCount++;
                if (useCount > 64) {
                    return false; // Depth / expansion recursion bomb
                }
            } else if (name == u"image") {
                // If an image tag is present, it must only use embedded data URIs
                for (const auto &attr : xml.attributes()) {
                    if (attr.name() == u"href" || attr.name() == u"xlink:href") {
                        QString val = attr.value().toString().trimmed();
                        if (!val.startsWith(QLatin1String("data:image/"))) {
                            return false; // Forbid external file / url reference
                        }
                    }
                }
            } else if (name == u"script") {
                return false; // Forbid executable script tags
            }

            // Check attributes for forbidden executable scripts or external references
            for (const auto &attr : xml.attributes()) {
                QString attrName = attr.name().toString().toLower();
                QString val = attr.value().toString().trimmed();
                QString lowerVal = val.toLower();

                if (lowerVal.contains(QLatin1String("javascript:"))) {
                    return false; // Forbid inline script execution
                }

                // Any href / xlink:href / src reference must be purely internal (#id) or embedded data:image/
                if (attrName == u"href" || attrName == u"src") {
                    if (!val.startsWith(QLatin1Char('#')) && !lowerVal.startsWith(QLatin1String("data:image/"))) {
                        return false; // Forbid external file / url reference
                    }
                }
            }
        }
    }

    return !xml.hasError();
}

QImage kynex_decode_icon(const QString &filePath, const QSize &requestedSize, int scale)
{
    QFileInfo fi(filePath);
    if (!fi.exists() || !fi.isFile() || fi.size() > 1024 * 1024) {
        return QImage();
    }

    QString suffix = fi.suffix().toLower();
    int effectiveScale = scale > 0 ? scale : 1;

    if (suffix == QLatin1String("png")) {
        QImageReader reader(filePath);
        reader.setAutoTransform(true);
        reader.setAllocationLimit(16);

        // Pre-decode header dimension check
        QSize headerSize = reader.size();
        if (!headerSize.isValid() || headerSize.width() <= 0 || headerSize.height() <= 0 ||
            headerSize.width() > 512 || headerSize.height() > 512) {
            return QImage(); // Decompression bomb protection
        }

        QImage img = reader.read();
        if (img.isNull()) {
            return QImage();
        }

        if (requestedSize.isValid() && (img.width() != requestedSize.width() || img.height() != requestedSize.height())) {
            img = img.scaled(requestedSize * effectiveScale, Qt::KeepAspectRatio, Qt::SmoothTransformation);
            img.setDevicePixelRatio(effectiveScale);
        }
        return img;
    } else if (suffix == QLatin1String("svg")) {
        QFile file(filePath);
        if (!file.open(QIODevice::ReadOnly)) {
            return QImage();
        }

        QByteArray data = file.readAll();
        if (!kynex_is_svg_safe(data)) {
            return QImage();
        }

        QSvgRenderer renderer;
        renderer.setOptions(QtSvg::NoOption);
        if (!renderer.load(data) || !renderer.isValid()) {
            return QImage();
        }

        QSize renderSize = requestedSize.isValid() ? requestedSize : renderer.defaultSize();
        if (!renderSize.isValid() || renderSize.width() <= 0 || renderSize.height() <= 0) {
            renderSize = QSize(32, 32);
        }

        // Clamp SVG target dimensions to max 512
        int w = std::min(512, std::max(1, renderSize.width()));
        int h = std::min(512, std::max(1, renderSize.height()));
        renderSize = QSize(w, h);

        QImage img(renderSize * effectiveScale, QImage::Format_ARGB32_Premultiplied);
        img.fill(Qt::transparent);

        QPainter painter(&img);
        renderer.render(&painter);
        painter.end();

        img.setDevicePixelRatio(effectiveScale);
        return img;
    }

    return QImage();
}

KynexIconResponse::KynexIconResponse(const QString &iconId, const QSize &requestedSize)
    : m_iconId(iconId), m_requestedSize(requestedSize)
{
    setAutoDelete(false);
}

KynexIconResponse::~KynexIconResponse() = default;

QQuickTextureFactory *KynexIconResponse::textureFactory() const
{
    if (m_image.isNull()) {
        return nullptr;
    }
    return QQuickTextureFactory::textureFactoryForImage(m_image);
}

QString KynexIconResponse::errorString() const
{
    return m_error;
}

void KynexIconResponse::run()
{
    // Parse query parameters if present in iconId (e.g. "firefox?size=48&scale=1")
    QString iconName = m_iconId;
    int size = m_requestedSize.isValid() ? m_requestedSize.width() : 32;
    int scale = 1;

    int queryIdx = m_iconId.indexOf(QLatin1Char('?'));
    if (queryIdx >= 0) {
        iconName = m_iconId.left(queryIdx);
        QUrlQuery query(m_iconId.mid(queryIdx + 1));
        if (query.hasQueryItem(QStringLiteral("size"))) {
            bool ok = false;
            int qsize = query.queryItemValue(QStringLiteral("size")).toInt(&ok);
            if (ok && qsize > 0 && qsize <= 512) {
                size = qsize;
            }
        }
        if (query.hasQueryItem(QStringLiteral("scale"))) {
            bool ok = false;
            int qscale = query.queryItemValue(QStringLiteral("scale")).toInt(&ok);
            if (ok && qscale > 0 && qscale <= 4) {
                scale = qscale;
            }
        }
    }

    if (size <= 0 || size > 512) {
        size = 32;
    }

    QByteArray nameUtf8 = iconName.toUtf8();
    char pathBuf[4096];
    std::memset(pathBuf, 0, sizeof(pathBuf));

    bool found = kynex_resolve_icon(nameUtf8.constData(), static_cast<uint32_t>(size), static_cast<uint32_t>(scale), pathBuf, sizeof(pathBuf));
    if (!found || pathBuf[0] == '\0') {
        m_error = QStringLiteral("Icon not found");
        Q_EMIT finished();
        return;
    }

    QString resolvedPath = QString::fromUtf8(pathBuf);
    QSize targetSize(size, size);
    m_image = kynex_decode_icon(resolvedPath, targetSize, scale);

    if (m_image.isNull()) {
        m_error = QStringLiteral("Failed to decode icon image");
        Q_EMIT finished();
        return;
    }

    Q_EMIT finished();
}

KynexIconProvider::KynexIconProvider()
    : QQuickAsyncImageProvider()
{
}

KynexIconProvider::~KynexIconProvider() = default;

QQuickImageResponse *KynexIconProvider::requestImageResponse(const QString &id, const QSize &requestedSize)
{
    auto *response = new KynexIconResponse(id, requestedSize);
    QThreadPool::globalInstance()->start(response);
    return response;
}

void kynex_register_image_provider(QQmlEngine *engine)
{
    if (engine) {
        kynex_init_security_limits();
        engine->addImageProvider(QStringLiteral("kynex-icon"), new KynexIconProvider());
    }
}

void kynex_register_image_provider_app(QQmlApplicationEngine &engine)
{
    kynex_register_image_provider(&engine);
}
