# Intake — Kubernetes-native Mosquitto (wersja redakcyjna)

> Wersja czytelna transkrypcji. Źródło: [intake-transcript.md](intake-transcript.md) (dosłowna transkrypcja, bez zmian).

Celem najbliższego zakresu zmian w Mosquitto jest maksymalne ułatwienie deploymentu i operowania brokerem w Kubernetesie oraz w klastrze kubernetesowym.

**1. Endpoint metryk zgodny z Prometheusem.**
Dziś wszystkie metryki są wystawiane wyłącznie na topikach systemowych. Chcemy wystawić te same metryki — nic więcej — w formacie zgodnym z Prometheusem.

**2. Wsparcie dla OpenTelemetry (propagacja kontekstu trace).**
Chodzi o przenoszenie trace parenta i spanów: gdy klient korzysta z messagingu opartego o MQTT 5.0, informacje o spanie zawsze wędrują w User Properties. Chcemy je przyjąć, przechować i propagować dalej, aby widoczny był cały przebieg distributed tracingu — od klienta do wszystkich słuchaczy, tak jak w każdym innym systemie messagingowym zgodnym z OpenTelemetry.

**3. Metryki OpenTelemetry.**
To samo co dla Prometheusa, ale zgodne ze standardem OpenTelemetry.

**4. Wszystkie te funkcje jako opt-in na etapie kompilacji.**
Każda z tych funkcji musi być sterowana flagą kompilacji. Wszystko jest opt-in: jeśli ktoś tego nie chce, deployment Mosquitto ma pozostać dokładnie taki sam i nie generować żadnego narzutu. Wsparcie dla OpenTelemetry i dla Prometheusa można włączać niezależnie od siebie — jedno, drugie albo oba.

**5. Strukturalne logowanie w JSON.**
Obecne logowanie Mosquitto chcemy zamienić na logowanie strukturalne w formacie JSON, tak aby logi trafiały w sensowny, ustrukturyzowany sposób do OpenTelemetry oraz do systemów zbierania logów, takich jak Loki czy Elasticsearch — co ma też ułatwić samo operowanie brokerem.
Dla logowania obowiązuje ta sama zasada opt-in: jeśli ktoś tego nie chce, nie ma po tym żadnego śladu.
