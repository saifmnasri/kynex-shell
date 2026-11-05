#pragma once

#include <QQuickWindow>
#include <QMargins>
#include <LayerShellQt/Window>

class LayerSurface : public QQuickWindow {
    Q_OBJECT
    Q_PROPERTY(Layer layer READ layer WRITE setLayer NOTIFY layerChanged)
    Q_PROPERTY(int anchors READ anchors WRITE setAnchors NOTIFY anchorsChanged)
    Q_PROPERTY(int exclusiveZone READ exclusiveZone WRITE setExclusiveZone NOTIFY exclusiveZoneChanged)
    Q_PROPERTY(QMargins margins READ margins WRITE setMargins NOTIFY marginsChanged)
    Q_PROPERTY(int leftMargin READ leftMargin WRITE setLeftMargin NOTIFY marginsChanged)
    Q_PROPERTY(int rightMargin READ rightMargin WRITE setRightMargin NOTIFY marginsChanged)
    Q_PROPERTY(int topMargin READ topMargin WRITE setTopMargin NOTIFY marginsChanged)
    Q_PROPERTY(int bottomMargin READ bottomMargin WRITE setBottomMargin NOTIFY marginsChanged)
    Q_PROPERTY(KeyboardInteractivity keyboardInteractivity READ keyboardInteractivity WRITE setKeyboardInteractivity NOTIFY keyboardInteractivityChanged)
    Q_PROPERTY(QString scope READ scope WRITE setScope NOTIFY scopeChanged)

public:
    enum Layer {
        LayerBackground = LayerShellQt::Window::LayerBackground,
        LayerBottom = LayerShellQt::Window::LayerBottom,
        LayerTop = LayerShellQt::Window::LayerTop,
        LayerOverlay = LayerShellQt::Window::LayerOverlay,
    };
    Q_ENUM(Layer)

    enum Anchor {
        AnchorNone = 0,
        AnchorTop = LayerShellQt::Window::AnchorTop,
        AnchorBottom = LayerShellQt::Window::AnchorBottom,
        AnchorLeft = LayerShellQt::Window::AnchorLeft,
        AnchorRight = LayerShellQt::Window::AnchorRight,
    };
    Q_ENUM(Anchor)

    enum KeyboardInteractivity {
        KeyboardInteractivityNone = LayerShellQt::Window::KeyboardInteractivityNone,
        KeyboardInteractivityExclusive = LayerShellQt::Window::KeyboardInteractivityExclusive,
        KeyboardInteractivityOnDemand = LayerShellQt::Window::KeyboardInteractivityOnDemand,
    };
    Q_ENUM(KeyboardInteractivity)

    explicit LayerSurface(QWindow *parent = nullptr);
    ~LayerSurface() override = default;

    Layer layer() const { return m_layer; }
    void setLayer(Layer layer);

    int anchors() const { return m_anchors; }
    void setAnchors(int anchors);

    int exclusiveZone() const { return m_exclusiveZone; }
    void setExclusiveZone(int zone);

    QMargins margins() const { return m_margins; }
    void setMargins(const QMargins &margins);

    int leftMargin() const { return m_margins.left(); }
    void setLeftMargin(int v);

    int rightMargin() const { return m_margins.right(); }
    void setRightMargin(int v);

    int topMargin() const { return m_margins.top(); }
    void setTopMargin(int v);

    int bottomMargin() const { return m_margins.bottom(); }
    void setBottomMargin(int v);

    KeyboardInteractivity keyboardInteractivity() const { return m_keyboardInteractivity; }
    void setKeyboardInteractivity(KeyboardInteractivity interactivity);

    QString scope() const { return m_scope; }
    void setScope(const QString &scope);

Q_SIGNALS:
    void layerChanged();
    void anchorsChanged();
    void exclusiveZoneChanged();
    void marginsChanged();
    void keyboardInteractivityChanged();
    void scopeChanged();

protected:
    void showEvent(QShowEvent *event) override;
    void resizeEvent(QResizeEvent *event) override;

private:
    void applyLayerShellSettings();

    Layer m_layer = LayerTop;
    int m_anchors = AnchorBottom | AnchorLeft | AnchorRight;
    int m_exclusiveZone = 0;
    QMargins m_margins;
    KeyboardInteractivity m_keyboardInteractivity = KeyboardInteractivityNone;
    QString m_scope;
};

