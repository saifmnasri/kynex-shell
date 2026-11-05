#include "layersurface.h"
#include <QScreen>

LayerSurface::LayerSurface(QWindow *parent)
    : QQuickWindow(parent)
{
    setFlags(flags() | Qt::FramelessWindowHint);
    setColor(Qt::transparent);
    resize(1280, 48);

    connect(this, &QWindow::widthChanged, this, [this](int w) {
        if (auto *ls = LayerShellQt::Window::get(this)) {
            int h = height() > 0 ? height() : 48;
            ls->setDesiredSize(QSize(w, h));
        }
    });
    connect(this, &QWindow::heightChanged, this, [this](int h) {
        if (auto *ls = LayerShellQt::Window::get(this)) {
            int w = width() > 0 ? width() : 1280;
            ls->setDesiredSize(QSize(w, h));
        }
    });
}

void LayerSurface::setLayer(Layer layer)
{
    if (m_layer != layer) {
        m_layer = layer;
        applyLayerShellSettings();
        Q_EMIT layerChanged();
    }
}

void LayerSurface::setAnchors(int anchors)
{
    if (m_anchors != anchors) {
        m_anchors = anchors;
        applyLayerShellSettings();
        Q_EMIT anchorsChanged();
    }
}

void LayerSurface::setExclusiveZone(int zone)
{
    if (m_exclusiveZone != zone) {
        m_exclusiveZone = zone;
        applyLayerShellSettings();
        Q_EMIT exclusiveZoneChanged();
    }
}

void LayerSurface::setMargins(const QMargins &margins)
{
    if (m_margins != margins) {
        m_margins = margins;
        applyLayerShellSettings();
        Q_EMIT marginsChanged();
    }
}

void LayerSurface::setLeftMargin(int v)
{
    if (m_margins.left() != v) {
        m_margins.setLeft(v);
        applyLayerShellSettings();
        Q_EMIT marginsChanged();
    }
}

void LayerSurface::setRightMargin(int v)
{
    if (m_margins.right() != v) {
        m_margins.setRight(v);
        applyLayerShellSettings();
        Q_EMIT marginsChanged();
    }
}

void LayerSurface::setTopMargin(int v)
{
    if (m_margins.top() != v) {
        m_margins.setTop(v);
        applyLayerShellSettings();
        Q_EMIT marginsChanged();
    }
}

void LayerSurface::setBottomMargin(int v)
{
    if (m_margins.bottom() != v) {
        m_margins.setBottom(v);
        applyLayerShellSettings();
        Q_EMIT marginsChanged();
    }
}

void LayerSurface::setKeyboardInteractivity(KeyboardInteractivity interactivity)
{
    if (m_keyboardInteractivity != interactivity) {
        m_keyboardInteractivity = interactivity;
        applyLayerShellSettings();
        Q_EMIT keyboardInteractivityChanged();
    }
}

void LayerSurface::setScope(const QString &scope)
{
    if (m_scope != scope) {
        m_scope = scope;
        applyLayerShellSettings();
        Q_EMIT scopeChanged();
    }
}

void LayerSurface::showEvent(QShowEvent *event)
{
    applyLayerShellSettings();
    QQuickWindow::showEvent(event);
}

void LayerSurface::resizeEvent(QResizeEvent *event)
{
    applyLayerShellSettings();
    QQuickWindow::resizeEvent(event);
}

void LayerSurface::applyLayerShellSettings()
{
    LayerShellQt::Window *lsWindow = LayerShellQt::Window::get(this);
    if (!lsWindow) {
        return;
    }

    lsWindow->setLayer(static_cast<LayerShellQt::Window::Layer>(m_layer));
    lsWindow->setAnchors(LayerShellQt::Window::Anchors::fromInt(m_anchors));
    lsWindow->setExclusiveZone(m_exclusiveZone);
    lsWindow->setMargins(m_margins);

    int w = width() > 0 ? width() : (screen() ? screen()->size().width() : 1280);
    int h = height() > 0 ? height() : 48;
    lsWindow->setDesiredSize(QSize(w, h));

    lsWindow->setKeyboardInteractivity(
        static_cast<LayerShellQt::Window::KeyboardInteractivity>(m_keyboardInteractivity));
    if (!m_scope.isEmpty()) {
        lsWindow->setScope(m_scope);
    }
}
