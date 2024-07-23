# TP2 Concurrentes

## Video presentacion

Para ver el video haga click [aqui](https://drive.google.com/file/d/1jqvpdIxZ7EVk55nawFf5jKFNel1yzoiT/view?usp=sharing)


### Comandos de ejecución

Para definir la maxima cantidad de Robots y de Screens en el programa se deberan modificar las variables en el archivo `config.rs`:
- `MAX_NUMBER_OF_SCREENS`: Cantidad maxima de pantallas
- `MAX_NUMBER_OF_ROBOTS`: Cantidad maxima de robots


## Screens

```
cargo run --bin screen <num_screen> <file_name>
```
El archivo debe estar en la carpeta `orders_samples`
## Robots 

```
cargo run --bin robot <num_robot>
```


## Arquitectura
El sistema está compuesto por dos aplicaciones distintas: Screen y Robot

![image](https://github.com/concurrentes-fiuba/2024-1c-tp2-freddogpt/assets/104704316/7e24795d-4ba7-42b9-a7f0-30be1f8a650c)


### Screen

Las Screens son el punto de comunicacion con los clientes. Son los encargados de procesar los pedidos recibidos, capturar el pago y enviárselo a la entidad organizadora. Una vez que un pedido se termina de preparar, la Screen efectúa el pago. A su vez, internamente las Screens se encargan de manejar la comunicación con el Robot Lider.

El nodo Screen tiene implementado los siguientes actores:

- `OrderReader`: Se encarga de recibir los pedidos por medio de un archivo, y procesarlos para luego enviárselos al Gateway de pagos. En caso de que una orden sea inválida, la misma es ignorada.
  
- `PaymentsGateway`: Es el encargado de capturar pagos antes de comenzar el pedido, y efectuarlo una vez preparado dicho pedido. Al momento de capturar cada pago, existe una propabilidad de que falle, y en ese caso el pedido es cancelado. Esto se realiza por medio de un commit de dos fases. Si la captura del pago es válida, se envía el mensaje *prepare* al ConnectionHandler para poder comenzar a preparar el pedido.

- `RobotConnectionHandler`: Se encarga de manejar la conexión con el Robot Líder. Luego de que un pedido sea capturado, este actor se encarga de enviarle al Robot Líder los pedidos por medio de sockets. Una vez que el pedido esté listo, recibe una confirmación de parte del Robot lider y ConnectionHandler envía dicha confimación al Gateway para finalmente confirmar el pago. También, puede recibir que el pedido falló o fue cancelado, y en ese caso también es avisado al Gateway para que aborte la compra. 

- `ScreenConnectionHandler`: Es el encargado de mantener las conexiones con las demás Screens. Esto permite mantener un Backup de los pedidos pendientes y finalizados, para que, en caso de que caiga una Screen, se puedan realizar los pedidos de la Screen caída y se puedan aceptar las ordenes finalizadas.


## Backup

Las Screens realizan backups entre pares, de manera que cada Screen solo guarda el backup de otra. Por ejemplo, la Screen A realiza sus backups en la Screen B, la Screen B realiza sus backups en la Screen C, y la Screen C guarda sus backups en la Screen A. De esta manera, evitamos tener un único punto de falla. Para darnos cuenta de que una Screen se cayo usamos un algoritmo por el cual se manda cada una cierta cantidad de segundos un mensaje entre las Screens (en forma de anillo) con un keep alive.


### Robots:

Los nodos Robots se encargan de servir el helado. Cada uno de ellos está conectado por medio de sockets a dos robots, generando un anillo. Uno de ellos es el líder, el cual tiene conexión directa con las Screens y es quien distribuye los pedidos a los demás robots.

El nodo Robot tiene implementado los siguientes actores:

-`ScreenConnectionHandler`: Mantiene las conexiones con las Screens. Recibe las ordenes y entrega los pedidos correspondientes. Este actor solo va a estar activo en el robot lider.

-`RobotConnectionHandler`: Es quien mantiene una conexión con sus robots adyacentes. De esta forma, puede recibir tokens de gustos de helado y enviarlos. Cada vez que se recibe un token, se envía un mensaje a OrderManager con el token recibido, este se encarga de decidir que hacer. Además, se posee un Timer para mantener cuándo fue la última vez que se sirvió un gusto de helado, ya que luego de cierta cantidad de tiempo, se puede haber perdido un gusto de helado y en ese caso se debe recuperar dicho gusto. En caso de que reciba un gusto de helado que no tenga cantidad suficiente para realizar el pedido, se envía abort al RobotConnectionHandler. Es importante recalcar que cada robot mantiene una referencia actualizada de los gustos de helado y la última cantidad recibida del token.

-`OrderManager`: Se encarga de gestionar el progreso de los pedidos y de verificar si se requiere un token de gusto de helado en el pedido actual.  Si no se necesita un token o el OrderPreparer está actualmente usando otro, se enviará el token a su robot adyacente. Una vez completado el pedido, envía un mensaje al RobotConnectionHandler para que el robot líder notifique a la pantalla que el pedido ha sido realizado o abortado. 

-`OrderPreparer`: Este actor es responsable de acceder a la sección crítica del gusto de helado y deducir la cantidad necesaria para el pedido.

## Backup

El robot líder realiza un respaldo de su información en dos robots adyacentes. Cada vez que se modifica la información en el líder, este envía un mensaje a sus dos robots adyacentes con el cambio correspondiente. Los demás robots no necesitan realizar copias de seguridad, ya que el líder posee toda la información necesaria para continuar con la ejecución normal en caso de fallos.

Para detectar si un robot ha fallado, utilizamos un algoritmo de supervisión mediante el cual se envían mensajes de "keep alive" entre los robots en forma de anillo a intervalos regulares. Si algún robot no responde a estos mensajes, se asume que ha fallado.

![image](https://github.com/concurrentes-fiuba/2024-1c-tp2-freddogpt/assets/104704316/2fa9cf9b-ded1-4681-b89a-daccaff50539)

### Gustos de Helado:

Cada gusto de helado será representado por un token que circulará entre los robots. Cuando un robot reciba un token, podrá servirse de ese gusto de helado. Si no lo necesita al gusto o esta actualmente sirviendo otro, enviará el token a su robot adyacente. Cada token contiene el nombre del gusto de helado y la cantidad disponible. Si la cantidad se agota, el token seguirá recorriendo los robots para avisarles de que no pueden preparar pedidos con ese gusto. 


## Interacciones entre procesos

Las comunicaciones entre los distintos procesos serán a través de Sockets TCP debido a que garantizan conexiones confiables entre nuestras entidades, asegurando que los paquetes enviados lleguen a su destino en orden y sin duplicados. Aunque este tipo de conexión tiene un costo mayor en comparación con UDP, ya que requiere mantener una conexión abierta entre las diferentes entidades y un Socket aceptador, las ventajas que ofrece son significativas.

Se utilizan Enums para definir los tipos de mensajes que se pueden enviar. Los mensajes se clasifican en dos enums principales: RobotMessage y ScreenMessage. Cada uno de estos enums contiene las distintas variantes de mensajes que un robot o una pantalla pueden mandar.

Para serializar y deserializar estos mensajes, utilizamos el crate serde_json, que permite convertir los mensajes a formato JSON y viceversa. El proceso de envío y recepción de mensajes funciona de la siguiente manera:

1. Serialización: Antes de enviar un mensaje a través del socket TCP, este se convierte a una cadena JSON utilizando serde_json. Esto asegura que el mensaje sea transmitido en un formato facil de interpretar. Una vez serializado, se añade un carácter de nueva línea (\n) al final del mensaje con el fin de indicar el final del mensaje y facilitar su correcta delimitación durante la recepción.

2. Envío: El mensaje serializado (ahora una cadena JSON seguida de \n) se envía a través del socket TCP.

3. Recepción: En el otro extremo del socket, el mensaje se lee hasta encontrar el carácter \n, que indica el final del mensaje completo.

4. Deserialización: Una vez recibido el mensaje, se elimina el carácter \n y la cadena JSON se deserializa utilizando serde_json. Esto permite reconstruir el mensaje original en su forma de enum, ya sea RobotMessage o ScreenMessage, junto con todos los atributos que ese mensaje específico pueda contener.
   
## Tipos de mensajes 
#### Entre Robots

Los robots se comunican entre si para pasarse los tokens de gustos de helado o para la eleccion de lider. Debido a esto creamos los siguientes mensajes:

#### Protocolo entre Robots Desconectados 

- `SetNextRobot`: Se envía para que el robot establezca su próximo robot

- `SetPreviousRobot`: Se envía para que el robot establezca su robot adyacente anterior.

#### Protocolo entre Robots Conectados

- `Election`: Se envía para comenzar la elección de líder.

- `NewLeader`: Este mensaje se envía a cada robot para que sepan quién es el nuevo líder.

- `PrepareOrder`: Este mensaje lo envía el líder a cada robot para que preparen un pedido

- `TokenTurn`: Es el mensaje que se envía entre robots para pasarse el token. Dicho mensaje contiene el sabor y la cantidad actual de helado disponible.
  
- `OrderPrepared`: Lo envía cada robot al líder para avisar que finalizó el pedido.
  
- `OrderAborted`: Se envía al robot líder cuando el robot no pudo preparar el pedido.


#### Entre Screens

Los posibles mensajes a enviar entre pantallas están relacionados a los backups:

- `TakeMyBackup`: Contiene el backup del screen adyacente, y el screen que lo recibe debe guardar en su backup dicha información.

#### Screens y Lider Robot
Las Screens pueden enviarle los siguientes mensajes al robot lider:

-`PrepareNewOrder`: Este mensaje contiene la información del pedido, para que el robot líder la reciba y algún robot la prepare.

Los Robots pueden enviar los siguientes mensajes:

-`OrderPrepared`: Junto con la información del pedido, se envía a la pantalla para que confirme el pedido una vez preparado.

-`OrderAborted`: Si un pedido no pudo ser preparado, se envía a la pantalla para que aborte el pedido.


### Casos de error

-`Se cae el robot`: El líder siempre mantiene información actualizada sobre el estado de los pedidos. En caso de que un robot se caiga, el líder agrega el pedido pendiente de ese robot al principio de la cola de órdenes a realizar.

-`Se cae el robot lider`: En caso de que el robot líder se caiga, algún robot adyacente lo detectará y disparará el algoritmo de anillo para la elección de un nuevo líder. Este algoritmo selecciona al nuevo líder utilizando como heurística el robot que tenga un backup y un ID mayor. De esta manera, aprovechamos la disposición en anillo de los robots y aplicamos el algoritmo de anillo. Finalmente, el nuevo líder envía su backup actualizado a sus robots adyacentes.

-`Se cae el token de gusto de helado`: En caso de que se pierda un token, el robot que lo descubra empezará a enviar una lista con la última cantidad vista por el robot y la pasará a los demás. Una vez que de toda la vuelta, el robot levantará un token de ese gusto con la menor cantidad de helado que un robot tenía referenciada en su lista propia.

-`Se cae una Screen`:  Las Screens mantienen backups entre pares. Gracias a esto, cuando se detecta la caída de una Screen, la Screen que tiene su backup la reemplaza. Esta Screen agrega al final de su lista de pedidos los pedidos pendientes de la Screen caída y además recibe todos los nuevos pedidos que se hubieran dirigido a la Screen anterior.


## Segunda Entrega 

Se realizaron las siguientes modificaciones respecto a la primera entrega:

### Screens


![image](https://github.com/concurrentes-fiuba/2024-1c-tp2-freddogpt/assets/104704316/05928d74-3672-4930-95aa-98a03cd0c448)


- Inicialmente, en el modelo de actores, se había establecido un actor llamado `ScreenConnectionHandler` para manejar las conexiones entre las pantallas en forma de anillo. Sin embargo, se decidió dividir las responsabilidades de la conexión con la screen adyacente siguiente y la screen adyacente anterior en dos actores diferentes:

  1. `ScreenConnectionListener`: Este actor es responsable de recibir mensajes de las pantallas adyacentes anteriores a él y también de gestionar las conexiones con el robot líder. A través de este actor, se reciben los    backups, se procesan las solicitudes específicas de las pantallas y se maneja la conexión con el robot líder.

  2. `ScreenConnectionSender`: Este actor se encarga de enviar los backups a screen siguiente y de enviar solicitudes a la misma para poder conectarse con el robot líder en caso de pérdida de conexión.

- Se agregaron los siguientes mensajes de comunicación:
  1. `RequestRobotLeaderConnection`: Si una screen se conecta luego de que el robot líder haya sido establecido, solicitará a su screen adyacente que le avise al robot líder que se conectó. De esta forma, el robot líder    se conectará a la nueva screen, manteniendo la consistencia de conexiones.
  2. `GiveMeThisScreenOrders`: En caso de que una pantalla se desconecte, la pantalla que tenía su backup asumirá la responsabilidad de sus pedidos. Para gestionar esto, enviará el mensaje `GiveMeThisScreenOrders` al   
  robot líder, notificándole que los pedidos en preparación que pertenecían a la pantalla desconectada ahora deben ser enviados a ella.

### Robots

- Decidimos que todos los robots, en lugar de solo dos, mantuvieran el respaldo del robot líder. Esto nos proporciona una mayor resiliencia en el sistema y además facilitó el proceso de programación de esta parte. 

- También cambiamos el método para detectar fallos en los robots. En lugar de usar KeepAlive como habíamos planteado inicialmente, optamos por utilizar las conexiones TCP para detectar si un robot se cae. De este modo, podemos detectar de manera asincrónica la caída de un robot, ya que el StreamHandler del actor de la conexión nos avisa.
 

- Se creo un actor `RobotLeader`, el cual se encarga del comportamiento del robot cuando es lider, es decir, comunicandose con las Screens, y con cada Robot para asignar y recibir pedidos. Este actor puede ser creado desde cero o desde un backup, de esta forma el pro
Usa dos actores para comunicarse con ambos:
  1. `LeaderToRobotConnection`: Se comunica por los siguientes mensajes:
      - `SendLeaderBackup`: Manda su backup a los robots
      - `SendNewOrder`: Le manda una nueva orden a un robot
      - `OrderComplete`: El robot le avisa que completo su orden
      - `OrderNotFinished`: El robot le avisa que no pudo completar su orden
       
  2. `LeaderToScreenConnection`: Utiliza los mensajes:
      - `OrderPrepared`: Le manda la orden lista a la pantalla
      - `OrderAborted`: Le dice a la pantalla que la orden fue abortada y por que
      - `PrepareNewOrder`: La pantalla le avisa al Lider que prepare un pedido nuevo
      - `RequestRobotToLeaderConnection`: Una pantalla le avisa al lider que se conecte con una nueva pantalla que recien se co 
      - `GiveMeThisScreenOrders`: Una pantalla le avisa al lider que desde ahora le de ella todas las ordenes que eran para esta otra pantalla 

- El actor `RobotConnectionHandler`, continua encargandose de las comunicaciones  con el lider, utilizando el actor `RobotToLeaderConnection` (manejando los mensajes explicados previamente), y con sus robots adyacentes.
  1. `RobotToRobot`:  Espera recibir los siguientes mensajes:
      - `TokenMessage`: Contiene un FlavorToken, que es un gusto de helado, el cual contiene el FlavorID de ese gusto y la cantidad actual.
      - `TokenBackupMsg`: Es el mensaje que se pasa en busca del valor mas actualizado, para luego recuperar el token y reincorporarlo al anillo.
      - `NewLeader`:  Recibe el ID del nuevo lider, resultado de una eleccion.
      - `NewElection`: Recibe la lista con los candidatos actuales.  
  
- Cuando un robot se une al ring lo hace por medio de un mensaje `JoinRing`, donde intenta conectarse a todos los robots del anillo, de no encontrar ningun otro robot activo, se autoproclama lider, y pasa a conectarse con las pantallas disponibles. Cuando un nuevo robot quiera unirse al anillo, reconocera a este robot como el lider y esperara recibir instrucciones de este.


![conexion](https://github.com/concurrentes-fiuba/2024-1c-tp2-freddogpt/assets/90098530/694f9034-cd43-4eaf-8b5b-399871f52b07)


- Ahora el `OrderManager` se encarga de manejar la logica de cada pedido y de contar con el Timer que informa si hay un token perdido. Este actor guarda en cada pasada, y cuando termina de servirse, los valores actualizados de cada token. Cuando se da cuenta que uno o mas tokens se encuentran perdidos, comienza el algoritmo de recuperacion, buscando el Robot que tenga el valor mas actualizado del sabor, es decir el que menor cantidad tenga. Al finalizar, el robot usa el backup para poner el token nuevamente en juego y poder servirse lo que necesita.
