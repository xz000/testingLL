using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class StartBattleButton : MonoBehaviour
{
    float countdown = 20;
    public TestMenu02 tm02;
    public Text ButtonText;
    void OnEnable()
    {
        GetComponent<Button>().interactable = false;
        countdown = 20;
    }

    private void Update()
    {
        countdown -= Time.unscaledDeltaTime;
        ButtonText.text = "Start (" + countdown.ToString("F1") + ")";
        if (countdown < 0)
        {
            if (GetComponent<Button>().interactable)
                ClickStart();
            else
            {
                gameObject.SetActive(false);
                tm02.ClickBackButton();
            }
        }
    }

    public void ClickStart()
    {
        GetComponent<Button>().interactable = false;
        gameObject.SetActive(false);
        countdown = 20;
        tm02.SenderSC.SendHello(4);
    }
}
