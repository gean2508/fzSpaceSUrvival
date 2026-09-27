pub mod fireworks;
pub mod lod;
pub mod particles;
pub mod scale;

use bevy::prelude::*;

/// Cuelga `child` de `parent` **sin reventar si `parent` ya no existe**.
///
/// EL CIERRE ALEATORIO TRAS UN RATO DE JUEGO SALÍA DE ACÁ. Los sistemas que
/// *decoran* entidades ajenas (sombra de suelo, tinte de luz, LOD, animación de
/// pose) las sacan de una `Query`, y una `Query` refleja el mundo **antes** de
/// que se apliquen los comandos del frame. Si en ese mismo frame el streamer de
/// chunks tiró la entidad —cosa que pasa a cada paso que da el jugador— el
/// comando de decoración se aplica sobre una entidad muerta, y en Bevy 0.15 eso
/// es un `panic!` (error B0003), no un aviso. El hook de pánico hace
/// `process::exit(1)`: el juego se cierra de golpe, sin diálogo, después de un
/// rato al azar. Cuanto más se camina o se vuela, antes cae.
///
/// La regla, entonces: **para tocar una entidad que no spawneaste vos en este
/// mismo sistema, usá `try_insert` (`remove` ya es tolerante) y esta función en
/// vez de `with_children`/`add_child`.**
///
/// Si el padre se murió, el hijo se va con él — si no, quedaría una entidad
/// huérfana suelta por cada sombra que llegó tarde, que es la fuga que este
/// arreglo NO quiere cambiar por el pánico.
pub fn try_add_child(commands: &mut Commands, parent: Entity, child: Entity) {
    commands.queue(move |world: &mut World| {
        if world.get_entity(parent).is_ok() {
            world.entity_mut(parent).add_child(child);
        } else {
            world.despawn(child);
            cuenta_padre_muerto();
        }
    });
}

/// `insert` tolerante Y **contado**, para los sistemas que decoran entidades
/// ajenas. Igual que `EntityCommands::try_insert`, pero deja constancia de cada
/// vez que la entidad ya no estaba: sin eso, cambiar `insert` por `try_insert`
/// tapa el problema sin decir nunca si existía. `quien` es la etiqueta que sale
/// en el log.
pub fn try_insert<B: Bundle>(commands: &mut Commands, e: Entity, quien: &'static str, bundle: B) {
    commands.queue(move |world: &mut World| match world.get_entity_mut(e) {
        Ok(mut ent) => {
            ent.insert(bundle);
        }
        Err(_) => cuenta_muerto(quien),
    });
}

fn cuenta_padre_muerto() {
    cuenta_muerto("sombra/padre");
}

/// Cuántas veces se quiso tocar una entidad ya muerta, por sitio. **Cada una de
/// estas era un cierre del juego** antes del arreglo, así que el contador es la
/// medida de cuánto valía: si en una sesión larga marca cero en todos los sitios,
/// el pánico venía de otro lado y hay que seguir buscando — no darlo por
/// arreglado. Se loguea la primera de cada sitio y después una de cada 100.
fn cuenta_muerto(quien: &'static str) {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static N: Mutex<Option<HashMap<&'static str, u64>>> = Mutex::new(None);
    let Ok(mut g) = N.lock() else { return };
    let mapa = g.get_or_insert_with(HashMap::new);
    let n = mapa.entry(quien).or_insert(0);
    *n += 1;
    if *n == 1 || *n % 100 == 0 {
        eprintln!("ha-ecs muerto quien={quien} n={n} (antes esto cerraba el juego)");
    }
}
