#pragma once

#include <QQuickAsyncImageProvider>
#include <QQuickImageResponse>
#include <QQuickTextureFactory>
#include <QRunnable>
#include <QThreadPool>
#include <QImage>
#include <QImageReader>
#include <QByteArray>
#include <QString>
#include <QSize>

class QQmlEngine;
class QQmlApplicationEngine;

/// Verifies that an SVG payload contains no external resource references,
/// external DTD/entity declarations, or pathological <use> expansion bombs.
bool kynex_is_svg_safe(const QByteArray &data);

/// Decodes an icon from disk enforcing hard dimension (512x512) and memory (16 MB) caps.
QImage kynex_decode_icon(const QString &filePath, const QSize &requestedSize, int scale);

/// Initializes Qt-wide security limits (e.g. QImageReader 16 MB allocation limit).
void kynex_init_security_limits();

/// Asynchronous image response executed off the GUI thread in QThreadPool.
class KynexIconResponse : public QQuickImageResponse, public QRunnable
{
    Q_OBJECT
public:
    KynexIconResponse(const QString &iconId, const QSize &requestedSize);
    ~KynexIconResponse() override;

    QQuickTextureFactory *textureFactory() const override;
    QString errorString() const override;

    void run() override;

private:
    QString m_iconId;
    QSize m_requestedSize;
    QImage m_image;
    QString m_error;
};

/// Asynchronous image provider for "image://kynex-icon/<name>?size=<N>&scale=<S>".
class KynexIconProvider : public QQuickAsyncImageProvider
{
public:
    KynexIconProvider();
    ~KynexIconProvider() override;

    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &requestedSize) override;
};

/// Registers KynexIconProvider into the QML engine.
void kynex_register_image_provider(QQmlEngine *engine);
void kynex_register_image_provider_app(QQmlApplicationEngine &engine);

