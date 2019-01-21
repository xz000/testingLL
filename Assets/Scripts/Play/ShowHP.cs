using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.UI;

public class ShowHP : MonoBehaviour
{
    //public Component bar;
    public GameObject MyHPGO;
    GameObject MyHP;
    Text MyHPText;

    // Use this for initialization
    void Start ()
    {
        Canvas TheCanvas = FindObjectOfType<Canvas>();
        MyHP = Instantiate(MyHPGO, TheCanvas.gameObject.transform);
        MyHPText = MyHP.gameObject.GetComponentInChildren<Text>();
        //GetComponent<StealthScript>().BarSR = MyHP;
    }
	
	// Update is called once per frame
	void Update ()
    {
        MyHP.transform.position = Camera.main.WorldToScreenPoint(transform.position + Vector3.up * 0.7f);
        MyHP.GetComponent<Slider>().value = (float)GetComponent<HPScript>().currentHP / (float)GetComponent<HPScript>().maxHP;
        MyHPText.text = Mathf.Round((float)GetComponent<HPScript>().currentHP * 10) / 10 + "/" + gameObject.GetComponent<HPScript>().maxHP;
    }

    private void OnDestroy()
    {
        Destroy(MyHP);
    }
}
