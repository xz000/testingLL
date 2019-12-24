using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class ShowMyInfo : MonoBehaviour
{
    public GameObject MyInfoText;
    GameObject MyInfoHP;
    Rigidbody2D rb2d;
    HPScript hps;
    bool notshow = false;

    // Use this for initialization
    void Start()
    {
        GameObject TheCanvas = GameObject.Find("Canvas2");
        MyInfoHP = GameObject.Instantiate(MyInfoText, TheCanvas.transform);
        rb2d = GetComponent<Rigidbody2D>();
        hps = GetComponent<HPScript>();
    }

    private void Update()
    {
        if(notshow)
            return;
        MyInfoHP.transform.position = Camera.main.WorldToScreenPoint(transform.position);
        MyInfoHP.GetComponent<Text>().text = hps.currentHP.ToString("F1");
    }

    private void OnDestroy()
    {
        Destroy(MyInfoHP);
    }

    public void DoNotShow()
    {
        MyInfoHP.GetComponent<Text>().text = "";
        notshow = true;
    }

    public void DoShow()
    {
        notshow = false;
        MyInfoHP.GetComponent<Text>().text = hps.currentHP.ToString("F1");
    }
}
