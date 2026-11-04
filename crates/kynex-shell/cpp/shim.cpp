#include "shim.h"
#include "layersurface.h"
#include <QtQml>

void kynex_register_core_types()
{
    qmlRegisterType<LayerSurface>("org.kynexos.core", 1, 0, "LayerSurface");
}

